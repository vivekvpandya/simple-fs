use bytes::BufMut;
use futures::TryStreamExt;
use std::convert::Infallible;
use warp::{
    http::StatusCode,
    multipart::{FormData, Part},
    Filter, Rejection, Reply,
};

#[derive(Debug)]
struct ServerError {
    message: String,
}

impl warp::reject::Reject for ServerError {}

#[tokio::main]
async fn main() {
    let upload_route = warp::path("files")
        .and(warp::path("v1"))
        .and(warp::post())
        .and(warp::path::param())
        .and(warp::multipart::form().max_length(5_000_000))
        .and_then(upload);
    let download_route = warp::path("files")
        .and(warp::path("v1"))
        .and(warp::get())
        .and_then(list_files);
    let delete_route = warp::path("files")
        .and(warp::path("v1"))
        .and(warp::path::param())
        .and(warp::delete())
        .and_then(delete);
    let router = upload_route
        .or(download_route)
        .or(delete_route)
        .recover(handle_rejection);
    #[cfg(debug_assertions)]
    println!("Server started at localhost:8080");
    warp::serve(router).run(([0, 0, 0, 0], 8080)).await;
}

async fn list_files() -> Result<impl Reply, Rejection> {
    let dir_name = format!("./files");
    let mut dir = tokio::fs::read_dir(dir_name).await.map_err(|e| {
        eprint!("error listing dir: {}", e);
        warp::reject::custom(ServerError {
            message: e.to_string(),
        })
    })?;
    let mut files = Vec::new();
    while let Some(child) = dir.next_entry().await.map_err(|_e| {
        warp::reject::custom(ServerError {
            message: _e.to_string(),
        })
    })? {
        files.push(child.file_name());
    }
    let list_str = format!("{:#?}", files);
    Ok(list_str)
}

async fn delete(filename: String) -> Result<impl Reply, Rejection> {
    let fname = format!("./files/{}", filename);
    tokio::fs::remove_file(&fname).await.map_err(|e| {
        eprint!("error removing file: {}", e);
        warp::reject::custom(ServerError {
            message: e.to_string(),
        })
    })?;
    #[cfg(debug_assertions)]
    println!("removed file: {}", filename);
    Ok(format!("{} deleted", filename))
}

async fn upload(param_file_name: String, form: FormData) -> Result<impl Reply, Rejection> {
    let parts: Vec<Part> = form.try_collect().await.map_err(|e| {
        eprintln!("form error: {}", e);
        warp::reject::custom(ServerError {
            message: e.to_string(),
        })
    })?;

    for p in parts {
        if p.name() == "file" {
            let value = p
                .stream()
                .try_fold(Vec::new(), |mut vec, data| {
                    vec.put(data);
                    async move { Ok(vec) }
                })
                .await
                .map_err(|e| {
                    eprintln!("reading file error: {}", e);
                    warp::reject::custom(ServerError {
                        message: e.to_string(),
                    })
                })?;

            let fname2 = format!("./files/{}", param_file_name);
            tokio::fs::write(&fname2, value).await.map_err(|e| {
                eprint!("error writing file: {}", e);
                warp::reject::custom(ServerError {
                    message: e.to_string(),
                })
            })?;
            #[cfg(debug_assertions)]
            println!("created file: {}", param_file_name);
        }
    }

    Ok("success")
}

async fn handle_rejection(err: Rejection) -> std::result::Result<impl Reply, Infallible> {
    let (code, message) = if err.is_not_found() {
        (StatusCode::NOT_FOUND, "Not Found".to_string())
    } else if err.find::<warp::reject::PayloadTooLarge>().is_some() {
        (StatusCode::BAD_REQUEST, "Payload too large".to_string())
    } else if let Some(e) = err.find::<ServerError>() {
        (StatusCode::BAD_REQUEST, e.message.clone())
    } else {
        eprintln!("unhandled error: {:?}", err);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error.".to_string(),
        )
    };

    Ok(warp::reply::with_status(message, code))
}
