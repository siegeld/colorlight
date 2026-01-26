//! Minimal HTTP/1.1 server — status page + REST API.

use core::fmt::Write;
use crate::hub75::OutputMode;
use crate::menu::{Animation, Context};

// ── Request Parser ──────────────────────────────────────────────

const MAX_REQUEST: usize = 512;

pub enum Method {
    Get,
    Post,
    Unknown,
}

enum ParseState {
    ReadingHeaders,
    ReadingBody { content_length: usize, body_start: usize },
    Complete,
}

pub struct HttpRequest {
    buf: [u8; MAX_REQUEST],
    len: usize,
    state: ParseState,
}

impl HttpRequest {
    pub fn new() -> Self {
        Self {
            buf: [0; MAX_REQUEST],
            len: 0,
            state: ParseState::ReadingHeaders,
        }
    }

    pub fn reset(&mut self) {
        self.len = 0;
        self.state = ParseState::ReadingHeaders;
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.state, ParseState::Complete)
    }

    /// Feed received bytes. Returns `true` when the full request is available.
    pub fn feed(&mut self, data: &[u8]) -> bool {
        let space = MAX_REQUEST - self.len;
        let n = data.len().min(space);
        self.buf[self.len..self.len + n].copy_from_slice(&data[..n]);
        self.len += n;

        match self.state {
            ParseState::ReadingHeaders => {
                if let Some(end) = find_header_end(&self.buf[..self.len]) {
                    let body_start = end + 4;
                    let cl = parse_content_length(&self.buf[..end]);
                    if cl > 0 && self.len < body_start + cl {
                        self.state = ParseState::ReadingBody { content_length: cl, body_start };
                    } else {
                        self.state = ParseState::Complete;
                    }
                } else if self.len >= MAX_REQUEST {
                    self.state = ParseState::Complete;
                }
            }
            ParseState::ReadingBody { content_length, body_start } => {
                if self.len >= body_start + content_length || self.len >= MAX_REQUEST {
                    self.state = ParseState::Complete;
                }
            }
            ParseState::Complete => {}
        }
        self.is_complete()
    }

    pub fn method(&self) -> Method {
        if self.len >= 4 && &self.buf[..4] == b"GET " {
            Method::Get
        } else if self.len >= 5 && &self.buf[..5] == b"POST " {
            Method::Post
        } else {
            Method::Unknown
        }
    }

    pub fn path(&self) -> &str {
        let start = match self.method() {
            Method::Get => 4,
            Method::Post => 5,
            Method::Unknown => return "/",
        };
        let mut end = start;
        while end < self.len && self.buf[end] != b' ' && self.buf[end] != b'\r' {
            end += 1;
        }
        core::str::from_utf8(&self.buf[start..end]).unwrap_or("/")
    }

    fn body_str(&self) -> &str {
        if let Some(end) = find_header_end(&self.buf[..self.len]) {
            let body_start = end + 4;
            if body_start < self.len {
                return core::str::from_utf8(&self.buf[body_start..self.len]).unwrap_or("");
            }
        }
        ""
    }
}

// ── Response Writer ─────────────────────────────────────────────

pub struct HttpResponse {
    pub data: heapless::Vec<u8, 2048>,
}

impl HttpResponse {
    pub fn new() -> Self {
        Self { data: heapless::Vec::new() }
    }

    fn push_str(&mut self, s: &str) {
        self.data.extend_from_slice(s.as_bytes()).ok();
    }

    fn ok_html(&mut self) {
        self.data.clear();
        self.push_str("HTTP/1.1 200 OK\r\nContent-Type: text/html;charset=utf-8\r\nConnection: close\r\n\r\n");
    }

    fn ok_json(&mut self) {
        self.data.clear();
        self.push_str("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n");
    }

    fn not_found(&mut self) {
        self.data.clear();
        self.push_str("HTTP/1.1 404 Not Found\r\nConnection: close\r\n\r\nNot Found");
    }

    fn bad_request(&mut self, msg: &str) {
        self.data.clear();
        self.push_str("HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
        self.push_str(msg);
    }
}

impl core::fmt::Write for HttpResponse {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.data.extend_from_slice(s.as_bytes()).map_err(|_| core::fmt::Error)
    }
}

// ── Route Dispatcher ────────────────────────────────────────────

pub fn handle_request(req: &HttpRequest, resp: &mut HttpResponse, ctx: &mut Context, ip: [u8; 4]) {
    match (req.method(), req.path()) {
        (Method::Get, "/")                     => page_status(resp, ctx, ip),
        (Method::Get, "/api/status")           => api_status(resp, ctx, ip),
        (Method::Get, "/api/layout")           => api_layout_get(resp, ctx),
        (Method::Post, "/api/layout")          => api_layout_post(req, resp, ctx),
        (Method::Post, "/api/layout/apply")    => api_layout_apply(resp, ctx),
        (Method::Get, "/api/display")          => api_display_get(resp, ctx),
        (Method::Post, "/api/display/on")      => api_display_on(resp, ctx),
        (Method::Post, "/api/display/off")     => api_display_off(resp, ctx),
        (Method::Post, "/api/display/pattern") => api_display_pattern(req, resp, ctx),
        (Method::Get, "/api/bitmap/stats")     => api_bitmap_stats(resp, ctx),
        _                                      => resp.not_found(),
    }
}

// ── HTML Status Page ────────────────────────────────────────────

fn page_status(resp: &mut HttpResponse, ctx: &mut Context, ip: [u8; 4]) {
    resp.ok_html();
    let m = &ctx.mac;
    let (w, len) = ctx.hub75.get_img_param();
    let h = if w > 0 { len / w as u32 } else { 0 };
    let l = &ctx.layout;
    let anim = match ctx.animation {
        Animation::None => "None",
        Animation::Rainbow { .. } => "Rainbow",
    };
    write!(resp, "\
<!DOCTYPE html><html><head>\
<meta charset=utf-8><link rel=icon href='data:,'>\
<title>Colorlight</title>\
<style>body{{font:14px monospace;margin:20px}}td{{padding:2px 8px}}</style>\
</head><body><h2>Colorlight Status</h2><table>").ok();
    write!(resp, "<tr><td>MAC</td><td>{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}</td></tr>",
        m[0], m[1], m[2], m[3], m[4], m[5]).ok();
    write!(resp, "<tr><td>IP</td><td>{}.{}.{}.{}</td></tr>",
        ip[0], ip[1], ip[2], ip[3]).ok();
    write!(resp, "<tr><td>Display</td><td>{}x{}</td></tr>", w, h).ok();
    write!(resp, "<tr><td>Grid</td><td>{}x{} ({}x{} virtual)</td></tr>",
        l.grid_cols, l.grid_rows, l.virtual_width(), l.virtual_height()).ok();
    write!(resp, "<tr><td>Panel</td><td>{}x{}</td></tr>",
        l.panel_width, l.panel_height).ok();
    write!(resp, "<tr><td>Animation</td><td>{}</td></tr>", anim).ok();
    write!(resp, "<tr><td>Bitmap frames</td><td>{}</td></tr>",
        ctx.bitmap_stats.frames_completed).ok();
    write!(resp, "</table><h3>Panels</h3><table>").ok();
    for (i, a) in l.assignments.iter().enumerate() {
        match a {
            Some((col, row)) => {
                write!(resp, "<tr><td>J{}</td><td>({},{})</td></tr>", i + 1, col, row).ok();
            }
            None => {
                write!(resp, "<tr><td>J{}</td><td>-</td></tr>", i + 1).ok();
            }
        }
    }
    write!(resp, "</table><h3>Pattern</h3>\
<select id=pat>\
<option>grid</option>\
<option>rainbow</option>\
<option>rainbow_anim</option>\
<option>white</option>\
<option>red</option>\
<option>green</option>\
<option>blue</option>\
</select> \
<button onclick=\"fetch('/api/display/pattern',{{method:'POST',headers:{{'Content-Type':'application/json'}},\
body:JSON.stringify({{name:document.getElementById('pat').value}})}}).then(r=>r.json()).then(j=>{{document.getElementById('msg').textContent=j.ok?'Loaded!':'Error'}}\
).catch(()=>{{document.getElementById('msg').textContent='Failed'}})\">Load</button> \
<span id=msg></span>\
<h3>API</h3><ul>\
<li><a href=/api/status>/api/status</a></li>\
<li><a href=/api/layout>/api/layout</a></li>\
<li><a href=/api/display>/api/display</a></li>\
<li><a href=/api/bitmap/stats>/api/bitmap/stats</a></li>\
</ul></body></html>").ok();
}

// ── JSON API Handlers ───────────────────────────────────────────

fn api_status(resp: &mut HttpResponse, ctx: &mut Context, ip: [u8; 4]) {
    resp.ok_json();
    let m = &ctx.mac;
    let (w, len) = ctx.hub75.get_img_param();
    let h = if w > 0 { len / w as u32 } else { 0 };
    let l = &ctx.layout;
    let anim = match ctx.animation {
        Animation::None => "none",
        Animation::Rainbow { .. } => "rainbow",
    };
    write!(resp, r#"{{"mac":"{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}","#,
        m[0], m[1], m[2], m[3], m[4], m[5]).ok();
    write!(resp, r#""ip":"{}.{}.{}.{}","#, ip[0], ip[1], ip[2], ip[3]).ok();
    write!(resp, r#""display_width":{},"display_height":{},"#, w, h).ok();
    write!(resp, r#""grid":"{}x{}","virtual_width":{},"virtual_height":{},"#,
        l.grid_cols, l.grid_rows, l.virtual_width(), l.virtual_height()).ok();
    write!(resp, r#""panel_width":{},"panel_height":{},"#,
        l.panel_width, l.panel_height).ok();
    write!(resp, r#""animation":"{}","bitmap_frames":{}}}"#,
        anim, ctx.bitmap_stats.frames_completed).ok();
}

fn api_layout_get(resp: &mut HttpResponse, ctx: &Context) {
    resp.ok_json();
    let l = &ctx.layout;
    write!(resp, r#"{{"grid":"{}x{}","panel_width":{},"panel_height":{},"#,
        l.grid_cols, l.grid_rows, l.panel_width, l.panel_height).ok();
    write!(resp, r#""virtual_width":{},"virtual_height":{},"panels":{{"#,
        l.virtual_width(), l.virtual_height()).ok();
    let mut first = true;
    for (i, a) in l.assignments.iter().enumerate() {
        if let Some((col, row)) = a {
            if !first { write!(resp, ",").ok(); }
            write!(resp, r#""J{}":"{},{}""#, i + 1, col, row).ok();
            first = false;
        }
    }
    write!(resp, "}}}}").ok();
}

fn api_layout_post(req: &HttpRequest, resp: &mut HttpResponse, ctx: &mut Context) {
    let body = req.body_str();
    if let Some(grid) = json_get_str(body, "grid") {
        if let Some((cols, rows)) = crate::layout::parse_grid_spec(grid) {
            ctx.layout.grid_cols = cols;
            ctx.layout.grid_rows = rows;
        }
    }
    for i in 1u8..=8 {
        let key = [b'J', b'0' + i];
        if let Ok(key_str) = core::str::from_utf8(&key) {
            if let Some(pos) = json_get_str(body, key_str) {
                if let Some((col, row)) = crate::layout::parse_pos_spec(pos) {
                    ctx.layout.assignments[(i - 1) as usize] = Some((col, row));
                }
            }
        }
    }
    resp.ok_json();
    write!(resp, r#"{{"ok":true,"grid":"{}x{}"}}"#,
        ctx.layout.grid_cols, ctx.layout.grid_rows).ok();
}

fn api_layout_apply(resp: &mut HttpResponse, ctx: &mut Context) {
    ctx.layout.apply(&mut ctx.hub75);
    resp.ok_json();
    write!(resp, r#"{{"ok":true,"virtual_width":{},"virtual_height":{}}}"#,
        ctx.layout.virtual_width(), ctx.layout.virtual_height()).ok();
}

fn api_display_get(resp: &mut HttpResponse, ctx: &mut Context) {
    resp.ok_json();
    let (w, len) = ctx.hub75.get_img_param();
    let h = if w > 0 { len / w as u32 } else { 0 };
    let mode = match ctx.hub75.get_mode() {
        OutputMode::FullColor => "fullcolor",
        OutputMode::Indexed => "indexed",
    };
    let anim = match ctx.animation {
        Animation::None => "none",
        Animation::Rainbow { .. } => "rainbow",
    };
    write!(resp, r#"{{"width":{},"height":{},"mode":"{}","animation":"{}"}}"#,
        w, h, mode, anim).ok();
}

fn api_display_on(resp: &mut HttpResponse, ctx: &mut Context) {
    ctx.hub75.on();
    resp.ok_json();
    write!(resp, r#"{{"ok":true,"display":"on"}}"#).ok();
}

fn api_display_off(resp: &mut HttpResponse, ctx: &mut Context) {
    ctx.hub75.off();
    resp.ok_json();
    write!(resp, r#"{{"ok":true,"display":"off"}}"#).ok();
}

fn api_display_pattern(req: &HttpRequest, resp: &mut HttpResponse, ctx: &mut Context) {
    let body = req.body_str();
    let name = match json_get_str(body, "name") {
        Some(n) => n,
        None => { resp.bad_request("missing \"name\""); return; }
    };
    let (w, len) = ctx.hub75.get_img_param();
    let h = if w > 0 { (len / w as u32) as u16 } else { 0 };
    if w == 0 || h == 0 {
        resp.bad_request("image params not set");
        return;
    }

    use crate::patterns;
    let mut anim = false;
    let ok = match name {
        "grid"    => { ctx.hub75.write_img_data(0, patterns::grid(w, h)); true }
        "rainbow" => { ctx.hub75.write_img_data(0, patterns::rainbow(w, h)); true }
        "rainbow_anim" => {
            ctx.hub75.write_img_data(0, patterns::animated_rainbow(w, h, 0));
            anim = true;
            true
        }
        "white" => { ctx.hub75.write_img_data(0, patterns::solid_white(w, h)); true }
        "red"   => { ctx.hub75.write_img_data(0, patterns::solid_red(w, h)); true }
        "green" => { ctx.hub75.write_img_data(0, patterns::solid_green(w, h)); true }
        "blue"  => { ctx.hub75.write_img_data(0, patterns::solid_blue(w, h)); true }
        _ => false,
    };

    if ok {
        ctx.animation = if anim {
            Animation::Rainbow { phase: 0 }
        } else {
            Animation::None
        };
        ctx.hub75.swap_buffers();
        ctx.hub75.set_mode(OutputMode::FullColor);
        ctx.hub75.on();
        resp.ok_json();
        write!(resp, r#"{{"ok":true,"pattern":"{}"}}"#, name).ok();
    } else {
        resp.bad_request("unknown pattern");
    }
}

fn api_bitmap_stats(resp: &mut HttpResponse, ctx: &Context) {
    resp.ok_json();
    let s = &ctx.bitmap_stats;
    write!(resp, r#"{{"packets_total":{},"packets_valid":{},"#,
        s.packets_total, s.packets_valid).ok();
    write!(resp, r#""bad_magic":{},"bad_header":{},"#,
        s.packets_bad_magic, s.packets_bad_header).ok();
    write!(resp, r#""frames_completed":{},"last_frame_id":{},"#,
        s.frames_completed, s.last_frame_id).ok();
    write!(resp, r#""last_chunk":"{}/{}","last_size":"{}x{}","last_data_len":{}}}"#,
        s.last_chunk_index, s.last_total_chunks,
        s.last_width, s.last_height, s.last_data_len).ok();
}

// ── Helpers ─────────────────────────────────────────────────────

fn find_header_end(buf: &[u8]) -> Option<usize> {
    if buf.len() < 4 { return None; }
    for i in 0..buf.len() - 3 {
        if &buf[i..i + 4] == b"\r\n\r\n" {
            return Some(i);
        }
    }
    None
}

fn parse_content_length(headers: &[u8]) -> usize {
    for pattern in &[b"Content-Length: " as &[u8], b"content-length: "] {
        if let Some(pos) = find_bytes(headers, pattern) {
            let start = pos + pattern.len();
            let mut end = start;
            while end < headers.len() && headers[end] >= b'0' && headers[end] <= b'9' {
                end += 1;
            }
            if let Ok(s) = core::str::from_utf8(&headers[start..end]) {
                return parse_usize(s).unwrap_or(0);
            }
        }
    }
    0
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() { return None; }
    for i in 0..=haystack.len() - needle.len() {
        if &haystack[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

fn parse_usize(s: &str) -> Result<usize, ()> {
    let mut r: usize = 0;
    if s.is_empty() { return Err(()); }
    for b in s.bytes() {
        if b < b'0' || b > b'9' { return Err(()); }
        r = r.checked_mul(10).ok_or(())?.checked_add((b - b'0') as usize).ok_or(())?;
    }
    Ok(r)
}

/// Extract a string value for `key` from simple JSON: `{"key":"value",...}`.
fn json_get_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let b = json.as_bytes();
    let kb = key.as_bytes();
    let len = b.len();
    let mut i = 0;
    while i < len {
        if b[i] == b'"' {
            let ks = i + 1;
            let ke = ks + kb.len();
            if ke < len && &b[ks..ke] == kb && b[ke] == b'"' {
                let mut p = ke + 1;
                while p < len && b[p] == b' ' { p += 1; }
                if p < len && b[p] == b':' {
                    p += 1;
                    while p < len && b[p] == b' ' { p += 1; }
                    if p < len && b[p] == b'"' {
                        let vs = p + 1;
                        let mut j = vs;
                        while j < len && b[j] != b'"' { j += 1; }
                        if j < len {
                            return core::str::from_utf8(&b[vs..j]).ok();
                        }
                    }
                }
            }
        }
        i += 1;
    }
    None
}
