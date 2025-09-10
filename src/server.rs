use std::io;
use may_minihttp::{HttpService, Request, Response};
use arc_swap::ArcSwap;
use crate::parser;

pub fn no_cache(res: &mut Response) {
    res.header("Cache-Control: no-cache");
    res.header("X-Content-Type-Options: nosniff");
    res.header("Accept-Ranges: bytes");
}

pub fn long_cache(res: &mut Response) {
    res.header("Cache-Control: public, max-age=31536000, immutable");
    res.header("X-Content-Type-Options: nosniff");
    res.header("Accept-Ranges: bytes");
}

#[derive(Clone)]
pub struct Page {
    pub content: std::sync::Arc<ArcSwap<parser::PreparedContent>>,
}

impl HttpService for Page {
    fn call(&mut self, req: Request, res: &mut Response) -> io::Result<()> {
        let path = req.path();
        let path = path.split_once('?').map(|(p, _)| p).unwrap_or(path);

        let prepared = self.content.load();

        match path {
            "/" | "/index.html" => {
                if let Some(ref html) = prepared.html_injected {
                    res.header("Content-Type: text/html; charset=utf-8");
                    res.body_vec(html.as_bytes().to_vec());
                } else {
                    res.status_code(404, "Not Found");
                    res.body_vec(b"No HTML content found".to_vec());
                }
                no_cache(res);
            }
            "/style.css" => {
                if let Some(ref css) = prepared.parsed.css {
                    res.header("Content-Type: text/css; charset=utf-8");
                    res.body_vec(css.as_bytes().to_vec());
                } else {
                    res.status_code(404, "Not Found");
                    res.body_vec(b"CSS not found".to_vec());
                }
                no_cache(res);
            }
            "/script.js" => {
                if let Some(ref js) = prepared.parsed.js {
                    res.header("Content-Type: text/javascript; charset=utf-8");
                    res.body_vec(js.as_bytes().to_vec());
                } else {
                    res.status_code(404, "Not Found");
                    res.body_vec(b"JavaScript not found".to_vec());
                }
                no_cache(res);
            }
            "/favicon.ico" => {
                res.status_code(204, "No Content");
                res.body_vec(Vec::new());
                long_cache(res);
            }
            _ => {
                res.status_code(404, "Not Found");
                res.body_vec(b"Page not found".to_vec());
                no_cache(res);
            }
        }

        Ok(())
    }
}
