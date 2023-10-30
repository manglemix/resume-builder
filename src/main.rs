#![feature(once_cell_try)]
use std::{sync::{mpsc::{self, SyncSender}, Arc}, fs::DirBuilder, path::PathBuf, hash::{Hash, Hasher}, cell::OnceCell, io::{self, Write}};

use anyhow::Context;
use fxhash::{FxHasher, FxHashSet};
use headless_chrome::Browser;
use page_scrapers::PageData;
use resume_gen::ResumeData;
use rust_bert::pipelines::keywords_extraction::{KeywordExtractionModel, Keyword};
use serde::Deserialize;
use tokio::task::JoinSet;
use tokio_rayon::rayon;
use url::Url;
use validator::Validate;
use crate::{page_scrapers::{PageDataSerde, DEFAULT_SCRAPERS, ScraperState, scrape_page}, resume_gen::{use_page_data, OUTPUT_PATH}};

mod page_scrapers;
mod resume_gen;

#[derive(Deserialize)]
struct Config {
    job_requirement_websites: Vec<Url>,
    #[serde(default)]
    omit_default_scrapers: Vec<String>,
    #[serde(default)]
    enable_optional_scrapers: Vec<String>,
    resume_data: ResumeData,
    resume_template_path: Option<String>
}

const CACHE_PATH: &str = ".cache/";

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    DirBuilder::new().recursive(true).create(CACHE_PATH).context("Failed to create cache directory. Do we have permissions?")?;

    let config = std::fs::read_to_string("config.toml").context("Failed to read config.toml. Does it exist? Do we have permissions?")?;
    let config: Config = toml::from_str(&config)?;

    config.resume_data.validate()?;
    let resume_data = Arc::new(config.resume_data);
    let resume_template_path = config.resume_template_path.map(Arc::new);

    let omit_default_scrapers: FxHashSet<String> = config.omit_default_scrapers.into_iter().collect();

    let enabled_scrapers: FxHashSet<String> = DEFAULT_SCRAPERS
        .into_iter()
        .filter_map(|x| if omit_default_scrapers.contains(x) {
                None
            } else {
                Some(x.to_string())
            })
        .chain(config.enable_optional_scrapers)
        .collect();
    let enabled_scrapers: &_ = Box::leak(Box::new(enabled_scrapers));

    let (keyword_extractor_sender, keyword_receiever) = mpsc::channel::<(Vec<String>, SyncSender<Vec<Vec<Keyword>>>)>();
    rayon::spawn(move || {
        let keyword_extraction_model = KeywordExtractionModel::new(Default::default()).expect("Keyword Extraction Model should have initialized");
        loop {
            let Ok((keywords, sender)) = keyword_receiever.recv() else { break };
            if sender.send(keyword_extraction_model.predict(&keywords).expect("Keyword Extraction Model should have worked")).is_err() {
                break
            }
        }
    });
    
    let browser = OnceCell::new();

    macro_rules! browser {
        () => {
            browser.get_or_try_init(|| Browser::default().context("Failed to start Headless Chrome. Do you have Chrome installed?"))?
        };
    }

    let mut scrape_tasks = JoinSet::<anyhow::Result<_>>::new();
    DirBuilder::new().recursive(true).create(OUTPUT_PATH).context("Failed to create resumes directory. Do we have permissions?")?;
    
    for url in config.job_requirement_websites {
        let url = Arc::new(url);
        let mut hasher = FxHasher::default();
        url.hash(&mut hasher);
        let hash = hasher.finish();
        let cached_file_path = PathBuf::from(CACHE_PATH).join(hash.to_string());

        if cached_file_path.try_exists().context(format!("Failed to check if a website has been cached. Do we have read permissions for {CACHE_PATH}?"))? {
            let tab = browser!().new_tab()?;
            let resume_data = resume_data.clone();
            let resume_template_path = resume_template_path.clone();
            scrape_tasks.spawn(async move {
                let bytes = tokio::fs::read(&cached_file_path).await.context(format!("Failed to read {cached_file_path:?}"))?;
                let page_data: PageDataSerde = bitcode::decode(&bytes).context(format!("Failed to deserialize {cached_file_path:?}. Consider deleting it."))?;
                let page_data = PageData::from(page_data);
                use_page_data(page_data, tab, resume_data, resume_template_path).await.context(format!("Failed to process {cached_file_path:?}"))
            });
            continue;
        }

        if url.scheme() == "http" {
            eprintln!("Warning!, you are attempting to scrape {} without https. Consider modifying the URL to use https instead.", url);
        }

        let tab = browser!().new_tab()?;
        let keyword_extractor_sender = keyword_extractor_sender.clone();
        let resume_data = resume_data.clone();
        let resume_template_path = resume_template_path.clone();

        scrape_tasks.spawn(async move {
            let url2 = url.clone();
            let (html, tab) = tokio_rayon::spawn(move || {
                tab.navigate_to(url2.as_str())?
                    .wait_until_navigated()?
                    .get_content()
                    .map(|x| (x, tab))
            }).await?;

            let state = ScraperState {
                html,
                url: url.clone(),
                keyword_extractor_sender,
                enabled_scrapers
            };
            
            let ((page_data, errors), state) = tokio_rayon::spawn(move || {
                (scrape_page(&state), state)
            }).await;
            let page_data_is_none = page_data.is_none();

            tokio::spawn(async move {
                let stdout = io::stdout();
                let mut stdout = stdout.lock();
                writeln!(stdout, "Finished scraping {}", state.url).unwrap();

                let stderr = io::stderr();
                let mut stderr = stderr.lock();

                for error in errors {
                    writeln!(stderr, "Error for {}: {error}", state.url).unwrap();
                }

                if page_data_is_none {
                    writeln!(stderr, "No Page Data!").unwrap();
                }
            });

            let Some(page_data) = page_data else {
                return Ok(())
            };
            let page_data = PageDataSerde::from(page_data);
            let encoded = bitcode::encode(&page_data).unwrap();
            let page_data = PageData::from(page_data);
            tokio::spawn(async move { tokio::fs::write(&cached_file_path, encoded).await.expect(&format!("{cached_file_path:?} should be writable")) });

            use_page_data(page_data, tab, resume_data, resume_template_path).await.context(format!("Failed to process {url}"))
        });
    }

    while let Some(result) = scrape_tasks.join_next().await {
        result??;
    }

    println!("Resumes completed successfully!");
    Ok(())
}
