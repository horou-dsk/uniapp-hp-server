use log::{info};
use actix_web::{web, HttpResponse, Error, HttpRequest};
use serde::{Serialize, Deserialize};
use std::io::{Write, Read};
use actix_multipart::{Multipart, Field};
use futures::{StreamExt, TryStreamExt};
use std::fs::{self, File};
use bytes::{BytesMut, BufMut};

const TOKEN: &str = "iQGhBUxcLRxE2xmwRJQ05a5YI8w1woWu";
const HOST: &str = "http://47.108.64.61:9699/update/projects/";

#[derive(Serialize, Deserialize)]
struct Info {
    username: String,
}

#[derive(Serialize, Deserialize)]
struct UpdateInfo {
    version: String,
    wgt_url: String,
    pkg_url: String,
}

#[derive(Serialize, Deserialize)]
struct ResultOk<T> {
    code: u16,
    data: T
}

impl<T> ResultOk<T> {
    fn new(data: T) -> Self {
        ResultOk { code: 200, data }
    }
}

#[derive(Serialize, Deserialize)]
struct ResultErr {
    code: u16,
    err_msg: String
}

impl ResultErr {
    fn new(code: u16, err_msg: &str) -> Self {
        ResultErr { code, err_msg: err_msg.to_string() }
    }
}


struct ResultJson;
impl ResultJson {
    fn ok<T>(data: T) -> ResultOk<T> {
        ResultOk::new(data)
    }

    fn err(code: u16, err_msg: &str) -> ResultErr {
        ResultErr::new(code, err_msg)
    }
}

fn get_version(req: HttpRequest) -> HttpResponse {
    // let qs = QString::from();
    // println!("{}", qs.get("project").unwrap());
    let project_name: String = req.match_info().query("project_name").parse().unwrap();
    let mut file = match File::open("./tmp/".to_string() + &*project_name + "/version.json") {
        Ok(file) => file,
        Err(_) => {
            let err = ResultJson::err(500, "没有找到项目");
            return HttpResponse::Ok().content_type("application/json;charset=utf-8")
                .body(serde_json::to_string_pretty(&err).unwrap());
        }
    };

    let mut version_json = String::new();
    file.read_to_string(&mut version_json).unwrap();
    let info: UpdateInfo = serde_json::from_str(&*version_json).unwrap();
    info!("项目 {} 获取版本号 {}", project_name, info.version);
    let info = ResultJson::ok(info);
    HttpResponse::Ok().json(info)
}

fn check_update() -> HttpResponse {
    // let qs = QString::from(req.query_string());
    let update_info = UpdateInfo {
        version: "1.0.0".to_string(),
        wgt_url: "http://www.baidu.com".to_string(),
        pkg_url: "http://www.google.com".to_string(),
    };
    HttpResponse::Ok().json(serde_json::to_string(&update_info).unwrap())
}

async fn get_field_chunk(mut field: Field) -> BytesMut {
    let mut b = BytesMut::new();
    while let Some(chunk) = field.next().await {
        b.put(chunk.unwrap())
    };
    b
}

async fn save_wgt(mut payload: Multipart) -> Result<HttpResponse, Error>{
    // iterate over multipart stream
    let mut token = None;
    let mut project_name = None;
    let mut version: Option<String> = None;
    let mut pkg_url = "".to_string();
    while let Ok(Some(mut field)) = payload.try_next().await {
        let content_type = field.content_disposition().unwrap();
        match content_type.get_filename() {
            Some(_) => {
                for has in vec![token.clone(), project_name.clone(), version.clone()] {
                    match has {
                        None => {
                            let err = ResultJson::err(500, "参数异常！");
                            return Ok(HttpResponse::Ok().json(serde_json::to_string(&err).unwrap()))
                        }
                        _ => {}
                    }
                }
                let project_name = project_name.unwrap();
                let filename = format!("{}.wgt", project_name);
                let dir_path = format!("./tmp/{}", project_name);
                fs::create_dir_all(dir_path.clone()).unwrap();
                let filepath = format!("{}/{}", dir_path.clone(), "version.json");
                let mut file = web::block(|| File::create(filepath))
                    .await
                    .unwrap();
                let version = version.unwrap();
                let version = UpdateInfo {
                    version,
                    wgt_url: format!("{}{}/{}", HOST, project_name, filename),
                    pkg_url,
                };
                let version = serde_json::to_string(&version).unwrap();
                web::block(move || file.write_all(version.as_bytes())).await.unwrap();
                let filepath = format!("{}/{}", dir_path, sanitize_filename::sanitize(&filename));
                println!("{}", filepath);
                // File::create is blocking operation, use threadpool
                let mut f = web::block(|| std::fs::File::create(filepath))
                    .await
                    .unwrap();
                // Field in turn is stream of *Bytes* object

                while let Some(chunk) = field.next().await {
                    let data = chunk.unwrap();
                    // filesystem operations are blocking, we have to use threadpool
                    f = web::block(move || f.write_all(&data).map(|_| f)).await.unwrap();
                };
                return Ok(HttpResponse::Ok().body("好了"))
            },
            None => {
                let chunk = get_field_chunk(field).await;// field.next().await.unwrap();
                let name = content_type.get_name().unwrap();
                let vec = chunk.to_vec();
                println!("{}", name);
                let value = String::from_utf8_lossy(&vec).to_string();
                match name {
                    "token" => {
                        if value.as_str() != TOKEN {
                            break
                        }
                        token = Some(value);
                    }
                    "project_name" => {
                        project_name = Some(value);
                    }
                    "version" => {
                        version = Some(value);
                    }
                    "pkg_url" => {
                        pkg_url = value;
                    }
                    _ => {}
                }
                // println!("{} = {}", name, );
            }
        };
    }
    let err = ResultJson::err(500, "参数异常！");
    return Ok(HttpResponse::Ok().json(serde_json::to_string(&err).unwrap()))
}

pub fn update_config(cfg: &mut web::ServiceConfig) {
    cfg.route("get_version/{project_name}", web::get().to(get_version))
        .route("check_update", web::get().to(check_update))
        .route("save_wgt", web::post().to(save_wgt))
        .service(actix_files::Files::new("/projects", "./tmp/"));
}