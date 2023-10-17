use anyhow::Context;
use serde::Deserialize;
use tokio::task::JoinSet;

use crate::page_scrapers::scrape_page;

mod page_scrapers;

#[derive(Deserialize)]
struct Config {
    job_requirement_websites: Vec<String>
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = std::fs::read_to_string("config.toml")?;
    let config: Config = toml::from_str(&config)?;

    let mut scrape_tasks = JoinSet::<anyhow::Result<_>>::new();
    let client = reqwest::Client::new();
    for url in config.job_requirement_websites {
        let client = client.clone();
        scrape_tasks.spawn(async move {
            let html = client.get(&url)
                .send()
                .await?
                .text()
                .await?;
            
            tokio_rayon::spawn(move || {
                let (page_data, errors) = scrape_page(&html);
                // .context(format!("Failed to scrape {url}"))
                todo!()
            }).await
        });
    }

    // let mut scrape_results = Vec::with_capacity(scrape_tasks.len());
    while let Some(result) = scrape_tasks.join_next().await {
        let page_data = result??;
        // scrape_results.push(result??);
    }

    println!("Resumes completed successfully!");
    Ok(())
}
