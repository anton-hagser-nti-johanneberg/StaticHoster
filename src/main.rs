use std::path::Path;

use actix_web::{
    http::StatusCode,
    middleware::Logger,
    web::{self},
    App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use git2::{Repository, ResetType};
use serde::{Deserialize, Serialize};

use crate::webhook::PushResponse;

pub mod webhook;

async fn delete(
    path: web::Path<(String, String, String)>,
    data: web::Data<(String, sled::Db, String)>,
) -> impl Responder {
    println!("Deleting website");
    let inner = path.into_inner();
    let repo_name = format!("{}-{}", &inner.1, &inner.2);

    if inner.0 != data.2 {
        return HttpResponse::new(StatusCode::NETWORK_AUTHENTICATION_REQUIRED);
    }

    if data
        .1
        .get(&repo_name)
        .expect("internal sled error")
        .is_none()
    {
        return HttpResponse::new(StatusCode::NOT_FOUND);
    }

    let repo: LocalRepo = serde_json::from_str(
        &String::from_utf8(
            data.1
                .get(&repo_name)
                .expect("failed getting repo")
                .expect("failed getting repo again")
                .to_vec(),
        )
        .expect("failed"),
    )
    .expect("internal server error");

    let path = format!("{}/repos/{}", data.0, repo.name);
    let path = Path::new(&path);
    std::fs::remove_dir_all(path).expect("failed to delete repo");

    HttpResponse::new(StatusCode::OK)
}

async fn serve_website(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    data: web::Data<(String, sled::Db, String)>,
) -> impl Responder {
    println!("Fetching website");
    let inner = path.into_inner();
    let repo_name = format!("{}-{}", &inner.0, &inner.1);

    let path = req.match_info().query("filename");
    if path.contains("..") || repo_name.contains("..") {
        return HttpResponse::new(StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS);
    }

    if data
        .1
        .get(&repo_name)
        .expect("internal sled error")
        .is_none()
    {
        return HttpResponse::new(StatusCode::NOT_FOUND);
    }

    let repo: LocalRepo = serde_json::from_str(
        &String::from_utf8(
            data.1
                .get(&repo_name)
                .expect("failed getting repo")
                .expect("failed getting repo again")
                .to_vec(),
        )
        .expect("failed"),
    )
    .expect("internal server error");

    let file_type;
    let content = match path {
        "" => {
            file_type = Some(mime_guess::from_path("index.html"));
            let path = format!("{}/repos/{}/index.html", data.0, repo.name);
            let path = Path::new(&path);
            std::fs::read(path).expect("could not read file")
        }
        _ => {
            file_type = Some(mime_guess::from_path(&path));
            let path = format!("{}/repos/{}/{}", data.0, repo.name, path);
            let path = Path::new(&path);
            std::fs::read(path).expect("could not read file")
        }
    };

    let file_type = file_type.unwrap_or_else(|| mime_guess::from_ext("txt"));
    HttpResponse::Ok()
        .content_type(file_type.first().unwrap().to_string())
        .body(content)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalRepo {
    name: String,
    branch: String,
}

async fn github(
    body: web::Bytes,
    app_data: web::Data<(String, sled::Db, String)>,
) -> impl Responder {
    let data = String::from_utf8(body.to_vec()).unwrap();
    println!("{}", (data));
    let response: PushResponse = serde_json::from_str(&data).unwrap();

    let url = response.repository.url;
    let name = response.repository.full_name.replace("/", "-");
    let volume_path = &app_data.0;

    match app_data.1.get(&name).unwrap() {
        Some(path) => {
            println!("Repo already exists");
            let path: LocalRepo =
                serde_json::from_str(&String::from_utf8(path.to_vec()).unwrap()).unwrap();

            // Do magic and pull it
            let repo = match Repository::open(format!("{}/repos/{}", volume_path, name)) {
                Ok(repo) => repo,
                Err(e) => panic!("Failed to open: {}", e),
            };

            repo.reset(
                &repo.revparse_single("HEAD").unwrap(),
                ResetType::Hard,
                None,
            )
            .unwrap();

            if let Err(e) = fast_forward(
                path.branch,
                Path::new(&format!("{}/repos/{}", volume_path, name)),
            ) {
                panic!("Failed to pull: {}", e)
            }
        }
        None => {
            println!("Repo does not exist, cloning");
            // Do magic and clone it
            let repo = Repository::clone(&url, format!("{}/repos/{}", volume_path, name))
                .expect("failed to clone repo");
            let branches = repo.branches(None).expect("no branches");

            let res = branches
                .filter(|x| {
                    x.as_ref()
                        .expect("branch error")
                        .0
                        .name()
                        .as_ref()
                        .unwrap()
                        .unwrap()
                        == "master"
                        || x.as_ref()
                            .expect("branch error")
                            .0
                            .name()
                            .as_ref()
                            .unwrap()
                            .unwrap()
                            == "main"
                })
                .collect::<Vec<_>>();

            app_data
                .1
                .insert(
                    &name,
                    serde_json::to_string(&LocalRepo {
                        name: name.clone(),
                        branch: res[0]
                            .as_ref()
                            .unwrap()
                            .0
                            .name()
                            .as_ref()
                            .unwrap()
                            .unwrap()
                            .to_string(),
                    })
                    .unwrap()
                    .as_bytes(),
                )
                .expect("could not commit to db");
        }
    }

    HttpResponse::new(StatusCode::ACCEPTED)
}

fn fast_forward(branch: String, path: &Path) -> Result<(), git2::Error> {
    let repo = Repository::open(path)?;

    repo.find_remote("origin")?.fetch(&[&branch], None, None)?;

    let fetch_head = repo.find_reference("FETCH_HEAD")?;
    let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;
    let analysis = repo.merge_analysis(&[&fetch_commit])?;
    if analysis.0.is_up_to_date() {
        Ok(())
    } else if analysis.0.is_fast_forward() {
        let refname = format!("refs/heads/{}", branch);
        let mut reference = repo.find_reference(&refname)?;
        reference.set_target(fetch_commit.id(), "Fast-Forward")?;
        repo.set_head(&refname)?;
        repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
    } else {
        Err(git2::Error::from_str("Fast-forward only!"))
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info,debug"));

    let volume_path = std::env::var_os("VOLUME_PATH")
        .unwrap()
        .into_string()
        .unwrap();

    let delete_password = std::env::var_os("DELETE_PASSWORD")
        .unwrap()
        .into_string()
        .unwrap();

    // create repos folder
    if let Err(e) = std::fs::create_dir(format!("{}/repos/", volume_path)) {
        if e.raw_os_error() != Some(17) && e.raw_os_error() != Some(183) {
            panic!("error: {:?}", e);
        };
    }

    // create db folder
    if let Err(e) = std::fs::create_dir(format!("{}/db/", volume_path)) {
        if e.raw_os_error() != Some(17) && e.raw_os_error() != Some(183) {
            panic!("error: {:?}", e);
        };
    }

    println!("Running");
    let data = web::Data::new((
        volume_path.clone(),
        sled::open(format!("{}/db/", volume_path)).unwrap(),
        delete_password.clone(),
    ));

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::new("%a %{User-Agent}i | %r"))
            .app_data(data.clone())
            .route(
                "/delete/{password}/{account}/{repo}/",
                web::get().to(delete),
            )
            .route(
                "/{account}/{repo}/{filename:.*}/",
                web::get().to(serve_website),
            )
            .route("/github/webhook/", web::post().to(github))
    })
    .bind(("0.0.0.0", 36000))?
    .run()
    .await
}
