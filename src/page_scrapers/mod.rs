use std::{ops::Add, sync::{mpsc::{self, SyncSender}, Arc}, hash::Hash};

use fxhash::FxHashSet;
use rust_bert::pipelines::keywords_extraction::Keyword;
use url::Url;

use crate::page_scrapers::workday::WorkdayScraper;

use self::simplify::SimplifyScraper;

mod simplify;
mod workday;


pub(super) const DEFAULT_SCRAPERS: [&str; 2] = [SimplifyScraper::NAME, WorkdayScraper::NAME];


macro_rules! scrape_page {
    ($state: expr, $scraper: ty, $($scrapers: ty),+) => {{
        let ((mut data1, mut errs1), (data2, mut errs2)) = tokio_rayon::rayon::join(
            || {
                scrape_page!($state, $scraper)
            },
            || {
                scrape_page!($state, $($scrapers)*)
            }
        );
        if let Some(data1_inner) = data1 {
            if let Some(data2_inner) = data2 {
                data1 = Some(data1_inner + data2_inner)
            } else {
                data1 = Some(data1_inner);
            }
        } else if let Some(data2_inner) = data2 {
            data1 = Some(data2_inner);
        }
        if errs1.capacity() > errs2.capacity() {
            errs1.append(&mut errs2);
        } else {
            errs2.append(&mut errs1);
            errs1 = errs2;
        }
        (data1, errs1)
    }};
    ($state: expr, $scraper: ty) => {
        if $state.enabled_scrapers.contains(<$scraper>::NAME) {
            match <$scraper>::scrape($state) {
                None => (None, vec![]),
                Some(Err(e)) => (None, vec![e]),
                Some(Ok(x)) => (Some(x), vec![])
            }
        } else {
            (None, vec![])
        }
        
    };
}


#[derive(Debug, bitcode::Encode, bitcode::Decode, Clone)]
pub(super) struct KeyWithData<K: Hash + Eq, V> {
    pub(super) key: K,
    pub(super) data: V
}


impl<K: Hash + Eq, V> Hash for KeyWithData<K, V> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}


impl<K: Hash + Eq, V> Eq for KeyWithData<K, V> { }
impl<K: Hash + Eq, V> PartialEq for KeyWithData<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}


pub(super) fn scrape_page(state: &ScraperState) -> (Option<PageData>, Vec<anyhow::Error>) {
    scrape_page!(state, SimplifyScraper, WorkdayScraper)
}


/// Useful information gathered from a website that can be used to generate a resume
#[derive(Debug, Clone)]
pub(crate) struct PageData {
    /// Keywords regarding the job that can be used to generate a resume tailored for the job
    /// 
    /// Keywords must be a noun, verb, or adjective. Prepositions, pronouns, etc, are not useful.
    pub(crate) keywords: FxHashSet<KeyWithData<String, f32>>,
    pub(crate) url: Arc<Url>,
    pub(crate) job_title: String,
    pub(crate) company: String
}


impl From<PageDataSerde> for PageData {
    fn from(value: PageDataSerde) -> Self {
        Self {
            keywords: value.keywords,
            url: Arc::new(value.url.parse().expect("Serialized URL should have been valid")),
            job_title: value.job_title,
            company: value.company
        }
    }
}


impl From<PageData> for PageDataSerde {
    fn from(value: PageData) -> Self {
        Self {
            keywords: value.keywords,
            url: value.url.to_string(),
            job_title: value.job_title,
            company: value.company
        }
    }
}


/// Useful information gathered from a website that can be used to generate a resume
#[derive(Debug, bitcode::Encode, bitcode::Decode, Clone)]
pub(super) struct PageDataSerde {
    /// Keywords regarding the job that can be used to generate a resume tailored for the job
    /// 
    /// Keywords must be a noun, verb, or adjective. Prepositions, pronouns, etc, are not useful.
    keywords: FxHashSet<KeyWithData<String, f32>>,
    url: String,
    job_title: String,
    company: String
}


impl Add for PageData {
    type Output = Self;

    fn add(mut self, rhs: Self) -> Self::Output {
        for other_k in rhs.keywords {
            if let Some(mut self_k) = self.keywords.take(&other_k) {
                self_k.data += other_k.data;
                self.keywords.insert(self_k);
            } else {
                self.keywords.insert(other_k);
            }
        }
        self
    }
}


pub(super) struct ScraperState {
    pub(super) html: String,
    pub(super) url: Arc<Url>,
    pub(super) keyword_extractor_sender: mpsc::Sender<(Vec<String>, SyncSender<Vec<Vec<Keyword>>>)>,
    pub(super) enabled_scrapers: &'static FxHashSet<String>
}


pub(super) struct PendingKeywords(mpsc::Receiver<Vec<Vec<Keyword>>>);


impl PendingKeywords {
    pub(super) fn get(self) -> Vec<Vec<Keyword>> {
        self.0.recv().unwrap()
    }
}


impl ScraperState {
    pub(super) fn get_scraper(&self) -> scraper::Html {
        scraper::Html::parse_document(&self.html)
    }

    pub(super) fn extract_keywords(&self, keywords: Vec<String>) -> PendingKeywords {
        let (sender, receiver) = mpsc::sync_channel(1);
        let _ = self.keyword_extractor_sender.send((keywords, sender));
        PendingKeywords(receiver)
    }

    pub(super) fn create_page_data(&self) -> PageData {
        PageData { keywords: Default::default(), url: self.url.clone(), job_title: String::new(), company: String::new() }
    }
}


pub(super) trait PageScraper {
    const NAME: &'static str;

    /// Scrapes the given html, which is retrieved from the given URL
    /// 
    /// Returns None if this web scraper is not applicable to the given website.
    /// Returns Some(Err(_)) if the web scraper should have worked but failed for whatever reason.
    /// Returns Some(Ok(PageData)) if the web scraper successfully colelcted data from the page.
    /// 
    /// PageData is allowed to be empty. It is also allowed for a PageScraper to scrape a website it 
    /// was not designed for if it will be able to produce no misleading keywords. Examples of misleading keywords
    /// are those that are collected from any section that is not pertaining to the job, such as a navbar or footer (exceptions do exist of course).
    fn scrape(state: &ScraperState) -> Option<anyhow::Result<PageData>>;
}
