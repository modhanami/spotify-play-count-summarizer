use std::collections::HashMap;
use std::env;
use std::error::Error;

use actix_web::{App, get, HttpRequest, HttpResponse, HttpServer, Responder, web};
use actix_web::dev::{Response, ResponseHead};
use chrono::prelude::*;
use rspotify::model::{FullTrack, PlayHistory, TrackId};
use serde::{Deserialize, Serialize};

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
struct ApiResponse<T> {
    data: T,
    last_updated: DateTime<Utc>,
}

#[derive(Deserialize)]
struct PlayCountQuery {
    t: Option<String>,
}

struct GitHubInfo {
    user: String,
    pat: String,
    repo: String,
    data_path: String,
}

#[actix_web::main]
async fn main() {
    let addr = ("localhost", 38080);
    println!("Listening on {}:{}", addr.0, addr.1);

    HttpServer::new(|| {
        App::new()
            .service(play_count)
    })
        .bind(addr).unwrap()
        .run()
        .await.expect("TODO: panic message");
}

#[get("/play-count")]
async fn play_count(query: web::Query<PlayCountQuery>) -> impl Responder {
    match query.t.as_ref().unwrap_or(&"".to_string()).as_str() {
        "last-30-days" => (),
        _ => return HttpResponse::BadRequest().body("Invalid query"),
    }

    let gh_info = match get_github_info() {
        Ok(info) => info,
        Err(e) => {
            println!("{}", e);
            return HttpResponse::InternalServerError().body("Failed to get GitHub info");
        }
    };
    let history = get_last_x_days_play_history(3, false, gh_info).await.unwrap();

    let mut summarized_track_play_count = summarize_play_counts(history);
    summarized_track_play_count.sort_by(|a, b| b.count.cmp(&a.count));

    HttpResponse::Ok().json(ApiResponse {
        data: summarized_track_play_count,
        last_updated: Utc::now(),
    })
}

fn get_github_info() -> Result<GitHubInfo, Box<dyn Error>> {
    let gh_user = env::var("GH_USER")?;
    let gh_pat = env::var("GH_PAT")?;
    let gh_repo = env::var("GH_REPO")?;
    let gh_data_path = env::var("GH_DATA_PATH").unwrap_or("data".to_string());

    Ok(GitHubInfo {
        user: gh_user,
        pat: gh_pat,
        repo: gh_repo,
        data_path: gh_data_path,
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

async fn get_last_x_days_play_history(x: usize, latest_first: bool, gh_info: GitHubInfo) -> Result<Vec<PlayHistory>, Box<dyn Error>> {
    let mut play_history = Vec::new();
    let mut date = Utc::now().date();

    for _ in 0..x {
        let mut history = get_play_history_by_date(&date, &gh_info).await?;

        if latest_first {
            // the items played_at is in ascending order, reverse it to get the last played item first
            history.items.reverse();
        }

        play_history.extend(history.items);
        date = date.pred();
    }

    Ok(play_history)
}

async fn get_play_history_by_date(date: &Date<Utc>, gh_info: &GitHubInfo) -> Result<History, Box<dyn Error>> {
    let filename = format!("{}-{:02}-{:02}.json", date.year(), date.month(), date.day());
    println!("getting play history on {}", filename);

    let octocrab = octocrab::OctocrabBuilder::new().personal_token(gh_info.pat.clone()).build()?;

    let content_path = format!("{}/{}", gh_info.data_path, filename);
    let content_items = octocrab.repos(gh_info.user.clone(), gh_info.repo.clone()).get_content().path(content_path).send().await?;
    let content = match content_items.items[0].decoded_content() {
        Some(c) => c,
        None => return Err("No content".into()),
    };
    let history = serde_json::from_str::<History>(&content)?;

    Ok(history)
}

