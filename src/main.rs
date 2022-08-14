use std::{env, fs};
use std::collections::HashMap;
use std::error::Error;

use chrono::prelude::*;
use octocrab::Octocrab;
use rspotify::model::{FullTrack, PlayHistory, TrackId};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

#[derive(Deserialize)]
struct History {
    items: Vec<PlayHistory>,
}

#[derive(Serialize, Clone)]
struct TrackPlayCount {
    track: FullTrack,
    count: u32,
}

#[derive(Serialize)]
struct FileContent<T> {
    data: T,
    last_updated: DateTime<Utc>,
}

struct GitHubInfo {
    pat: String,
    gist_id: String,
    data_path: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let days = match env::var("DAYS") {
        Ok(days) => days.parse::<usize>()?,
        Err(_) => 1,
    };
    let gh_info = get_github_info()?;

    let history = get_last_x_days_play_history(days, &gh_info.data_path, false).await?;
    let mut summarized_track_play_count = summarize_play_counts(history);
    summarized_track_play_count.sort_by(|a, b| b.count.cmp(&a.count));

    let file_content = FileContent {
        data: summarized_track_play_count,
        last_updated: Utc::now(),
    };

    let file_content_json_pretty = serde_json::to_string(&file_content)?;
    let file_name = format!("last-{}-days-play-counts.json", days);
    let octocrab = create_octocrab(gh_info.pat)?;
    write_to_gist(&file_name, &file_content_json_pretty, &gh_info.gist_id, &octocrab).await;

    let elapsed = start.elapsed();
    println!("took {:?}", elapsed);

    Ok(())
}

fn create_octocrab(pat_token: String) -> Result<Octocrab, Box<dyn Error>> {
    match octocrab::OctocrabBuilder::new().personal_token(pat_token).build() {
        Ok(octocrab) => Ok(octocrab),
        Err(e) => Err(e.into()),
    }
}

async fn write_to_gist(file_name: &str, file_content: &str, gist_id: &str, octocrab: &Octocrab) {
    match octocrab.gists()
        .update(gist_id)
        .file(file_name)
        .with_content(file_content)
        .send()
        .await {
        Ok(_) => {
            println!("successfully updated gist");
        }
        Err(e) => {
            eprintln!("failed to update gist: {:?}", e);
        }
    }
}

fn get_github_info() -> Result<GitHubInfo, Box<dyn Error>> {
    let data_path = env::var("GH_DATA_PATH").unwrap_or("data".to_string());
    let pat = env::var("GH_PAT")?;
    let gist_id = env::var("GH_GIST_ID")?;

    Ok(GitHubInfo {
        pat,
        gist_id,
        data_path,
    })
}

fn summarize_play_counts(history: Vec<PlayHistory>) -> Vec<TrackPlayCount> {
    let mut track_id_to_track_play_counts: HashMap<TrackId, TrackPlayCount> = HashMap::new();
    for item in history {
        let track_id = match item.track.id.clone() {
            Some(id) => id,
            None => continue,
        };

        let entry = track_id_to_track_play_counts.entry(track_id).or_insert(TrackPlayCount {
            track: item.track,
            count: 0,
        });

        entry.count += 1;
    }

    let track_play_counts: Vec<TrackPlayCount> = track_id_to_track_play_counts.values().cloned().collect();
    track_play_counts
}

async fn get_last_x_days_play_history(days: usize, data_path: &str, latest_first: bool) -> Result<Vec<PlayHistory>, Box<dyn Error>> {
    let mut play_history = Vec::new();
    let mut date = Utc::now().date();

    for _ in 0..days {
        match get_play_history_by_date(&date, data_path).await {
            Ok(mut history) => {
                println!("got {} play history items for {}", history.items.len(), date);

                if latest_first {
                    // the items played_at is in ascending order, reverse it to get the last played item first
                    history.items.reverse();
                }

                play_history.extend(history.items);
            }
            Err(e) => {
                eprintln!("failed to get play history for date {}: {:?}", date, e);
            }
        };

        date = date.pred();
    }

    Ok(play_history)
}

async fn get_play_history_by_date(date: &Date<Utc>, data_path: &str) -> Result<History, Box<dyn Error>> {
    let filename = format!("{}-{:02}-{:02}.json", date.year(), date.month(), date.day());
    let path = format!("{}/{}", data_path, filename);
    let file_content = fs::read_to_string(&path)?;
    let history: History = serde_json::from_str(&file_content)?;

    Ok(history)
}

