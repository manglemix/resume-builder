use std::ops::AddAssign;

use fxhash::FxHashSet;
use url::Url;

use self::simplify::SimplifyScraper;

mod simplify;


macro_rules! scrape_page {
    ($html: expr, $scraper: ty, $($scrapers: ty),+) => {{
        let (a, b) = tokio_rayon::rayon::join(
            || {
                <$scraper>::scrape($html)
            },
            || {
                scrape_page!($html, $($scrapers)*)
            }
        );
        let mut a = a?;
        let b = b?;
        a += b;
        Ok(a)
    }};
    ($html: expr, $scraper: ty) => {
        <$scraper>::scrape($html)
    };
}


pub(crate) fn scrape_page(html: &str) -> (Option<PageData>, Vec<anyhow::Error>) {
    // scrape_page!(html, SimplifyScraper)
    todo!()
}


/// Useful information gathered from a website that can be used to generate a resume
pub(crate) struct PageData {
    /// Keywords regarding the job that can be used to generate a resume tailored for the job
    /// 
    /// Keywords must be a noun, verb, or adjective. Prepositions, pronouns, etc, are not useful.
    pub(crate) keywords: FxHashSet<Box<str>>
}


impl AddAssign for PageData {
    fn add_assign(&mut self, rhs: Self) {
        self.keywords.extend(rhs.keywords);
    }
}


pub(crate) trait PageScraper {
    /// Scrapes the given html, which is retrieved from the given URL
    /// 
    /// Returns None if this web scraper is not applicable to the given website.
    /// Returns Some(Err(_)) if the web scraper should have worked but failed for whatever reason.
    /// Returns Some(Ok(PageData)) if the web scraper successfully colelcted data from the page.
    /// 
    /// PageData is allowed to be empty. It is also allowed for a PageScraper to scrape a website it 
    /// was not designed for if it will be able to produce no misleading keywords. Examples of misleading keywords
    /// are those that are collected from any section that is not pertaining to the job, such as a navbar or footer (exceptions do exist of course).
    fn scrape(html: &str, url: &Url) -> Option<anyhow::Result<PageData>>;
}
