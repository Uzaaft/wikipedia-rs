extern crate hyper;
extern crate url;
extern crate serde_json;

use std::result;
use std::io;
use std::io::Read;
use std::cmp::PartialEq;

use hyper::{Client, Url};
use hyper::header::UserAgent;

const LANGUAGE_URL_MARKER:&'static str = "{language}";

macro_rules! results {
    ($data: expr, $query_field: expr) => {
        // There has to be a better way to write the following code
        try!(
            $data.as_object()
            .and_then(|x| x.get("query"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get($query_field))
            .and_then(|x| x.as_array())
            .ok_or(Error::JSONPathError)
            ).into_iter().filter_map(|i|
                i.as_object()
                .and_then(|i| i.get("title"))
                .and_then(|s| s.as_string().map(|s| s.to_owned()))
                ).collect()
    }
}


#[derive(Debug)]
pub enum Error {
    UrlError(url::ParseError),
    HyperError(hyper::error::Error),
    HTTPError(hyper::client::response::Response),
    IOError(io::Error),
    JSONError(serde_json::error::Error),
    JSONPathError,
    InvalidParameter(String),
}

impl std::convert::From<url::ParseError> for Error {
    fn from(e: url::ParseError) -> Self {
        Error::UrlError(e)
    }
}

impl std::convert::From<hyper::error::Error> for Error {
    fn from(e: hyper::error::Error) -> Self {
        Error::HyperError(e)
    }
}

impl std::convert::From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::IOError(e)
    }
}

impl std::convert::From<serde_json::error::Error> for Error {
    fn from(e: serde_json::error::Error) -> Self {
        Error::JSONError(e)
    }
}

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub struct Wikipedia {
    pub pre_language_url:String,
    pub post_language_url:String,
    pub user_agent:String,
    pub language:String,
    pub search_results:u32,
}

impl Default for Wikipedia {
    fn default() -> Self {
        Wikipedia {
            pre_language_url: "https://".to_owned(),
            post_language_url: ".wikipedia.org/w/api.php".to_owned(),
            user_agent: "wikipedia (https://github.com/seppo0010/wikipedia-rs)".to_owned(),
            language: "en".to_owned(),
            search_results: 10,
        }
    }
}

impl Wikipedia {
    pub fn base_url(&self) -> String {
        format!("{}{}{}", self.pre_language_url, self.language, self.post_language_url)
    }

    pub fn set_base_url(&mut self, base_url: &str) {
        let index = match base_url.find(LANGUAGE_URL_MARKER) {
            Some(i) => i,
            None => {
                self.pre_language_url = base_url.to_owned();
                self.language = "".to_owned();
                self.post_language_url = "".to_owned();
                return;
            }
        };
        self.pre_language_url = base_url[0..index].to_owned();
        self.post_language_url = base_url[index+LANGUAGE_URL_MARKER.len()..].to_owned();
    }

    fn query(&self, url: Url) -> Result<serde_json::Value> {
        let client = Client::new();
        let mut response = try!(client.get(url)
            .header(UserAgent(self.user_agent.clone()))
            .send());

        if !response.status.is_success() {
            return Err(Error::HTTPError(response));
        }

        let mut response_str = String::new();
        try!(response.read_to_string(&mut response_str));

        Ok(try!(serde_json::from_str(&*response_str)))
    }

    fn search_url(&self, query: &str) -> Result<Url> {
        let mut url = try!(Url::parse(&*self.base_url()));
        let results = &*format!("{}", self.search_results);
        url.set_query_from_pairs(vec![
                ("list", "search"),
                ("srprop", ""),
                ("srlimit", results),
                ("srsearch", query),
                ("continue", ""),
                ("format", "json"),
                ("action", "query"),
                ].into_iter());
        Ok(url)
    }

    pub fn search(&self, query: &str) -> Result<Vec<String>> {
        let url = try!(self.search_url(query));
        let data = try!(self.query(url));

        Ok(results!(data, "search"))
    }

    fn geosearch_url(&self, latitude: f64, longitude: f64, radius: u16) -> Result<Url> {
        if latitude < -90.0 || latitude > 90.0 {
            return Err(Error::InvalidParameter("latitude".to_string()))
        }
        if longitude < -180.0 || longitude > 180.0 {
            return Err(Error::InvalidParameter("longitude".to_string()))
        }
        if radius < 10 || radius > 10000 {
            return Err(Error::InvalidParameter("radius".to_string()))
        }
        let mut url = try!(Url::parse(&*self.base_url()));
        let results = &*format!("{}", self.search_results);
        url.set_query_from_pairs(vec![
                ("list", "geosearch"),
                ("gsradius", &*format!("{}", radius)),
                ("gscoord", &*format!("{}|{}", latitude, longitude)),
                ("gslimit", results),
                ("format", "json"),
                ("action", "query"),
                ].into_iter());
        Ok(url)
    }

    pub fn geosearch(&self, latitude: f64, longitude: f64, radius: u16) -> Result<Vec<String>> {
        let url = try!(self.geosearch_url(latitude, longitude, radius));
        let data = try!(self.query(url));

        Ok(results!(data, "geosearch"))
    }

    fn random_url(&self, count: u8) -> Result<Url> {
        let mut url = try!(Url::parse(&*self.base_url()));
        url.set_query_from_pairs(vec![
                ("list", "random"),
                ("rnnamespace", "0"),
                ("rnlimit", &*format!("{}", count)),
                ("continue", ""),
                ("format", "json"),
                ("action", "query"),
                ].into_iter());
        Ok(url)
    }

    pub fn random(&self) -> Result<Option<String>> {
        let url = try!(self.random_url(1));
        let data = try!(self.query(url));
        let r:Vec<String> = results!(data, "random");
        Ok(r.into_iter().next())
    }

    pub fn random_count(&self, count: u8) -> Result<Vec<String>> {
        let url = try!(self.random_url(count));
        let data = try!(self.query(url));
        Ok(results!(data, "random"))
    }

    pub fn page_from_title<'a>(&'a self, title: String) -> Page<'a> {
        Page::from_title(self, title)
    }
}

#[derive(Debug)]
enum TitlePageId {
    Title(String),
    PageId(String),
    TitleAndPageId(String, String),
}

impl TitlePageId {
    fn query_param(&self) -> (String, String) {
        match *self {
            TitlePageId::Title(ref s) => ("titles".to_owned(), s.clone()),
            TitlePageId::TitleAndPageId(ref s, _) => ("titles".to_owned(), s.clone()),
            TitlePageId::PageId(ref s) => ("pageids".to_owned(), s.clone()),
        }
    }
}

#[derive(Debug)]
pub struct Page<'a> {
    wikipedia: &'a Wikipedia,
    identifier: TitlePageId,
}

impl<'a> Page<'a> {
    pub fn from_title(wikipedia: &'a Wikipedia, title: String) -> Page {
        Page { wikipedia: wikipedia, identifier: TitlePageId::Title(title) }
    }

    pub fn from_pageid(wikipedia: &'a Wikipedia, pageid: String) -> Page {
        Page { wikipedia: wikipedia, identifier: TitlePageId::PageId(pageid) }
    }

    pub fn get_content(&self) -> Result<String> {
        let mut url = try!(Url::parse(&*self.wikipedia.base_url()));
        let qp = self.identifier.query_param();
        let params = vec![
            ("prop", "extracts|revisions"),
            ("explaintext", ""),
            ("rvprop", "ids"),
            ("format", "json"),
            ("action", "query"),
            (&*qp.0, &*qp.1),
        ];
        url.set_query_from_pairs(params.into_iter());

        let q = try!(self.wikipedia.query(url));
        let pages = try!(q
            .as_object()
            .and_then(|x| x.get("query"))
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("pages"))
            .and_then(|x| x.as_object())
            .ok_or(Error::JSONPathError));
        let pageid = match pages.keys().next() {
            Some(p) => p,
            None => return Err(Error::JSONPathError),
        };
        Ok(try!(try!(pages.get(pageid)
            .and_then(|x| x.as_object())
            .and_then(|x| x.get("extract"))
            .ok_or(Error::JSONPathError))
            .as_string()
            .ok_or(Error::JSONPathError))
            .to_owned())

    }
}

impl<'a> PartialEq<Page<'a>> for Page<'a> {
    fn eq(&self, other: &Page) -> bool {
        match self.identifier {
            TitlePageId::Title(ref t1) => match other.identifier {
                TitlePageId::Title(ref t2) => t1 == t2,
                TitlePageId::TitleAndPageId(ref t2, _) => t1 == t2,
                TitlePageId::PageId(_) => false,
            },
            TitlePageId::TitleAndPageId(ref t1, ref p1) => match other.identifier {
                TitlePageId::Title(ref t2) => t1 == t2,
                TitlePageId::TitleAndPageId(ref t2, _) => t1 == t2,
                TitlePageId::PageId(ref p2) => p1 == p2,
            },
            TitlePageId::PageId(ref p1) => match other.identifier {
                TitlePageId::Title(_) => false,
                TitlePageId::TitleAndPageId(_, ref p2) => p1 == p2,
                TitlePageId::PageId(ref p2) => p1 == p2,
            },
        }
    }
}

#[test]
fn base_url() {
    let mut wikipedia = Wikipedia::default();
    assert_eq!(wikipedia.base_url(), "https://en.wikipedia.org/w/api.php");
    wikipedia.language = "es".to_owned();
    assert_eq!(wikipedia.base_url(), "https://es.wikipedia.org/w/api.php");

    wikipedia.set_base_url("https://hello.{language}.world/");
    assert_eq!(wikipedia.base_url(), "https://hello.es.world/");

    wikipedia.set_base_url("https://hello.world/");
    assert_eq!(wikipedia.base_url(), "https://hello.world/");
}

#[test]
fn search_url() {
    let wikipedia = Wikipedia::default();
    assert_eq!(&*format!("{}", wikipedia.search_url("hello world").unwrap()),
            "https://en.wikipedia.org/w/api.php?list=search&srprop=&srlimit=10&srsearch=hello+world&continue=&format=json&action=query");
}

#[test]
fn search() {
    let wikipedia = Wikipedia::default();
    let results = wikipedia.search("hello world").unwrap();
    assert!(results.len() > 0);
    assert!(results.contains(&"\"Hello, World!\" program".to_owned()));
}

#[test]
fn geosearch_url() {
    let wikipedia = Wikipedia::default();
    assert_eq!(&*format!("{}", wikipedia.geosearch_url(-34.603333, -58.381667, 10).unwrap()),
            "https://en.wikipedia.org/w/api.php?list=geosearch&gsradius=10&gscoord=-34.603333%7C-58.381667&gslimit=10&format=json&action=query");
}

#[test]
fn geosearch() {
    let wikipedia = Wikipedia::default();
    let results = wikipedia.geosearch(-34.603333, -58.381667, 10).unwrap();
    assert!(results.len() > 0);
    assert!(results.contains(&"Buenos Aires".to_owned()));
}

#[test]
fn random_url() {
    let wikipedia = Wikipedia::default();
    assert_eq!(&*format!("{}", wikipedia.random_url(10).unwrap()),
            "https://en.wikipedia.org/w/api.php?list=random&rnnamespace=0&rnlimit=10&continue=&format=json&action=query");
}

#[test]
fn random() {
    let wikipedia = Wikipedia::default();
    wikipedia.random().unwrap().unwrap();
}

#[test]
fn random_count() {
    let wikipedia = Wikipedia::default();
    assert_eq!(wikipedia.random_count(3).unwrap().len(), 3);
}

#[test]
fn page_content() {
    let wikipedia = Wikipedia::default();
    let page = wikipedia.page_from_title("Parkinson's law of triviality".to_owned());
    assert!(page.get_content().unwrap().contains("bikeshedding"));
}
