use log::{info};
use actix_web::{web, HttpResponse, Error, HttpRequest};
use serde::{Serialize, Deserialize};
use std::io::{Read};
use actix_multipart::{Multipart, Field};
use futures::{StreamExt, TryStreamExt};
use std::fs::{File};
use tokio::fs::{self as afs, File as AsyncFile, OpenOptions};
use bytes::{BytesMut, BufMut, Bytes, Buf};
use qstring::QString;
use regex::Regex;
use std::collections::HashMap;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use std::option::Option::Some;

const TOKEN: &str = "iQGhBUxcLRxE2xmwRJQ05a5YI8w1woWu";

lazy_static! {
    static ref HOST: String = {
        let mut host = String::new();
        let mut file = File::open("./HOST.txt").unwrap();
        file.read_to_string(&mut host).unwrap();
        format!("http://{}:22336/update/projects/", host.trim())
    };
}
// const HOST: &str = "http://:9699/";

#[derive(Serialize, Deserialize)]
struct Info {
    username: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct UpdateInfo {
    version: String,
    wgt_url: String,
    pkg_url: String,
    update_log: String,
}

#[derive(Serialize, Deserialize)]
struct ResultOk<T> {
    code: u16,
    data: T,
}

impl<T> ResultOk<T> {
    fn new(data: T) -> Self {
        ResultOk { code: 200, data }
    }
}

#[derive(Serialize, Deserialize)]
struct ResultErr {
    code: u16,
    err_msg: String,
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

// mod http_result {
//     use actix_web::HttpResponse;
//     use crate::route::update::ResultJson;
//
//     fn ok<T>(data: T) -> HttpResponse {
//         HttpResponse::Ok().json(ResultJson::ok(data))
//     }
// }

async fn read_versions(path: &str) -> Result<Vec<UpdateInfo>, tokio::io::Error> {
    let mut version_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .await?;
    let mut json_str = String::new();
    version_file.read_to_string(&mut json_str).await?;
    Ok(match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => Vec::new()
    })
}

async fn write_versions<T>(path: &str, v: &T) -> Result<(), tokio::io::Error>
    where
        T: ?Sized + Serialize,
{
    let mut version_file = OpenOptions::new().write(true).truncate(true).open(path).await?;
    version_file.write_all(serde_json::to_string(v)?.as_bytes()).await?;
    Ok(())
}

fn get_version(req: HttpRequest) -> HttpResponse {
    let qs = QString::from(req.query_string());
    // println!("{}", qs.get("project").unwrap());
    let project_name: String = req.match_info().query("project_name").parse().unwrap();
    let platform = qs.get("platform").unwrap_or("没得");
    let version = match qs.get("version") {
        Some(v) => v.to_string(),
        None => return HttpResponse::Ok().json(ResultJson::err(400, "参数错误！"))
    };
    let project_filename = match platform {
        "ios" => format!("{}-ios", project_name),
        _ => project_name.clone()
    };
    let mut file = match File::open("./tmp/".to_string() + &*project_filename + "/version.json") {
        Ok(file) => file,
        Err(_) => {
            let err = ResultJson::err(500, "没有找到项目");
            return HttpResponse::Ok().content_type("application/json;charset=utf-8")
                .body(serde_json::to_string_pretty(&err).unwrap());
        }
    };

    let mut version_json = String::new();
    file.read_to_string(&mut version_json).unwrap();
    let infos: Vec<UpdateInfo> = serde_json::from_str(&*version_json).unwrap();
    let mut info = None;
    for ui in &infos {
        if info.is_some() {
            info = Some(ui);
            if !ui.pkg_url.is_empty() {
                break
            }
        } else if ui.version > version {
            info = Some(ui);
        }
    }
    // let info = infos.iter()
    //     .find_map(|v| if v.version == version { Some(v) } else { None });
    match info {
        Some(info) => {
            info!("项目 {} 获取版本号 {}", project_filename, info.version);
            let info = ResultJson::ok(info);
            HttpResponse::Ok().json(info)
        }
        None => {
            HttpResponse::Ok().json(ResultJson::err(500, "没有更新可用！"))
        }
    }
}

fn check_update() -> HttpResponse {
    // let qs = QString::from(req.query_string());
    let update_info = UpdateInfo {
        update_log: "".to_string(),
        version: "1.0.0".to_string(),
        wgt_url: "http://www.baidu.com".to_string(),
        pkg_url: "http://www.google.com".to_string(),
    };
    HttpResponse::Ok().json(serde_json::to_string(&update_info).unwrap())
}

async fn get_field_chunk(mut field: Field) -> Bytes {
    let mut b = BytesMut::new();
    while let Some(chunk) = field.next().await {
        b.put(chunk.unwrap())
    };
    b.to_bytes()
}

async fn save_update(map: HashMap<String, String>, filed: Option<Bytes>) -> Result<(), Box<dyn std::error::Error>> {
    let project_name = &map["project_name"];
    let version = &map["version"];
    let update_log = &map["update_log"];
    let project_filename = match project_name.as_str() {
        "ios" => format!("{}-ios", project_name),
        _ => project_name.to_string()
    };
    let pkg_url = map.get("pkg_url");
    let dir_path = format!("./tmp/{}", project_filename);
    afs::create_dir_all(&dir_path).await.unwrap();

    let wgt_url = match pkg_url {
        Some(_) => "".to_string(),
        None => {
            if let Some(field) = filed {
                let filename = format!("{}.wgt", version);
                let filepath = format!("{}/{}", dir_path, sanitize_filename::sanitize(&filename));
                info!("热更新wgt保存地址 = {}", filepath);
                let mut f = AsyncFile::create(filepath).await?;
                f.write_all(&field).await?;
                // while let Some(chunk) = field.next().await {
                //     let data = chunk.unwrap();
                //     // filesystem operations are blocking, we have to use threadpool
                //     f.write_all(&data).await?;
                //     // f = web::block(move || f.write_all(&data).map(|_| f)).await.unwrap();
                // };
                format!("{}{}/{}", *HOST, project_filename, filename)
            } else {
                panic!("读取更新文件流错误")
            }
        }
    };
    let filepath = format!("{}/{}", dir_path, "version.json");
    let mut versions = read_versions(&filepath).await?;
    let update_info = UpdateInfo {
        version: version.clone(),
        wgt_url,
        pkg_url: pkg_url.unwrap_or(&"".to_string()).to_string(),
        update_log: update_log.clone(),
    };
    let info = versions.iter_mut()
        .find_map(|v| if v.version == update_info.version { Some(v) } else { None });
    if let Some(info) = info {
        *info = update_info;
    } else {
        versions.push(update_info);
    }
    versions.sort_by(|a, b| a.version.cmp(&b.version));
    write_versions(&filepath, &versions).await?;
    Ok(())
}

async fn save_wgt(mut payload: Multipart) -> Result<HttpResponse, Error> {
    let mut file_field = None;
    let mut map = HashMap::new();
    while let Ok(Some(field)) = payload.try_next().await {
        let content_type = field.content_disposition().unwrap();
        match content_type.get_filename() {
            Some(_) => {
                file_field = Some(get_field_chunk(field).await);
            }
            None => {
                let chunk = get_field_chunk(field).await;// field.next().await.unwrap();
                let name = content_type.get_name().unwrap();
                let vec = chunk.to_vec();
                let value = String::from_utf8_lossy(&vec).to_string();
                if name == "token" && value.as_str() != TOKEN {
                    return Ok(HttpResponse::Ok().json(ResultJson::err(500, "参数错误")));
                }
                map.insert(name.to_string(), value);
            }
        };
    }
    if map.get("token").is_none() || map.get("token").unwrap() != TOKEN {
        return Ok(HttpResponse::Ok().json(ResultJson::err(400, "参数错误！")))
    }
    Ok(match save_update(map, file_field).await {
        Ok(_) => HttpResponse::Ok().json(ResultJson::ok("上传更新成功！")),
        Err(_) => HttpResponse::Ok().json(ResultJson::err(500, "上传更新失败！"))
    })
}

async fn delete_wgt(req: HttpRequest) -> HttpResponse {
    let qs = QString::from(req.query_string());
    let token = qs.get("token").unwrap_or("");
    let version = qs.get("version").unwrap_or("");
    if token != TOKEN || version == "" {
        return HttpResponse::Ok().json(ResultJson::err(400, "参数异常"));
    }
    let project_name = match qs.get("project_name") {
        Some(name) => name,
        _ => {
            return HttpResponse::Ok().json(ResultJson::err(500, "请输入项目名"))
        }
    };
    let re = Regex::new(r"^[a-zA-Z-_]+$").unwrap();
    if !re.is_match(project_name) {
        return HttpResponse::Ok().json(ResultJson::err(400, "参数异常"))
    }
    let platform = qs.get("platform").unwrap_or("");
    let project_filename = match platform {
        "ios" => format!("{}-ios", project_name),
        _ => project_name.to_string()
    };
    let filename = format!("{}.wgt", version);
    let dir_path = format!("./tmp/{}", project_filename);
    let version_path = format!("{}/{}", dir_path, "version.json");
    let wgt_path = format!("{}/{}", dir_path, sanitize_filename::sanitize(&filename));
    let mut versions = read_versions(&version_path).await.unwrap();
    let (mut delete, mut pkg) = (false, false);
    for index in 0..versions.len() {
        if versions[index].version == version {
            if !versions[index].pkg_url.is_empty() {
                pkg = true;
            }
            versions.remove(index);
            delete = true;
            break;
        }
    }
    if delete {
        write_versions(&version_path, &versions).await.unwrap();
        if pkg || afs::remove_file(wgt_path).await.is_ok() {
            return HttpResponse::Ok().json(ResultJson::ok("删除成功"));
        }
    }
    HttpResponse::Ok().json(ResultJson::err(500, "删除失败"))
}

pub fn update_config(cfg: &mut web::ServiceConfig) {
    cfg.route("get_version/{project_name}", web::get().to(get_version))
        .route("check_update", web::get().to(check_update))
        .route("save_wgt", web::post().to(save_wgt))
        .route("delete", web::get().to(delete_wgt))
        .service(actix_files::Files::new("/projects", "./tmp/"));
}