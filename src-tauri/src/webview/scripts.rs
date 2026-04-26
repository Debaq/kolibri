use tauri::{
    webview::{PageLoadEvent, WebviewBuilder},
    Emitter, Manager, Runtime,
};

#[cfg(target_os = "linux")]
use webkit2gtk::WebView;

pub const NAV_SPOOF: &str = r#"
(function () {
  if (window.__KOLIBRI_NAV__) return;
  window.__KOLIBRI_NAV__ = true;
  try {
    var def = function (prop, value) {
      try { Object.defineProperty(navigator, prop, { get: function () { return value; }, configurable: true }); } catch (e) {}
    };
    def('platform', 'Linux x86_64');
    def('vendor', 'Google Inc.');
    def('vendorSub', '');
    def('product', 'Gecko');
    def('productSub', '20030107');
    def('appName', 'Netscape');
    def('appCodeName', 'Mozilla');
    def('appVersion', '5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36');
    def('maxTouchPoints', 0);
  } catch (e) {}
})();
"#;

pub fn unread_watcher_script(svc_id: &str) -> String {
    format!(
        r#"(function () {{
  if (window.__KOLIBRI_UNREAD__) return;
  window.__KOLIBRI_UNREAD__ = true;
  var SID = "{sid}";
  var last = -1;
  function parse(t) {{
    if (!t) return 0;
    var m = t.match(/\((\d+)\+?\)/) || t.match(/^(\d+)\s/) || t.match(/\[(\d+)\]/) || t.match(/•\s?(\d+)/);
    if (m) return parseInt(m[1], 10) || 0;
    if (/^\s*[\*•!●]/.test(t)) return 1;
    return 0;
  }}
  function send(n) {{
    if (n === last) return;
    last = n;
    try {{
      var inv = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke;
      if (inv) inv('emit_unread', {{ serviceId: SID, count: n }});
    }} catch (e) {{}}
  }}
  function tick() {{ send(parse(document.title)); }}
  function attach() {{
    var t = document.querySelector('title');
    if (!t) {{ setTimeout(attach, 500); return; }}
    new MutationObserver(tick).observe(t, {{ childList: true, characterData: true, subtree: true }});
    tick();
  }}
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', attach);
  else attach();
  setInterval(tick, 5000);
}})();
"#,
        sid = svc_id.replace('"', "\\\"")
    )
}

/// Aplica los init scripts y on_page_load comunes a un builder de webview de servicio.
pub fn apply_common<R: Runtime>(
    mut builder: WebviewBuilder<R>,
    svc_id: &str,
) -> WebviewBuilder<R> {
    builder = builder.initialization_script(NAV_SPOOF);
    builder = builder.initialization_script(&unread_watcher_script(svc_id));
    let id = svc_id.to_string();
    builder = builder.on_page_load(move |webview: tauri::Webview<R>, payload| {
        let phase = match payload.event() {
            PageLoadEvent::Started => "started",
            PageLoadEvent::Finished => "finished",
        };
        let _ = webview
            .app_handle()
            .emit("kolibri:page_load", (id.clone(), phase));
    });
    builder
}

#[cfg(target_os = "linux")]
pub fn install_filedrop_bridge(view: &WebView) {
    use base64::Engine as _;
    use gtk::gdk::DragContext;
    use gtk::prelude::WidgetExt;
    use gtk::SelectionData;
    use webkit2gtk::{gio, WebViewExt};

    view.connect_drag_data_received(
        move |w: &WebView, _ctx: &DragContext, x: i32, y: i32, data: &SelectionData, info: u32, _time: u32| {
        // info==2 corresponde a target text/uri-list (lista de archivos).
        if info != 2 {
            return;
        }
        let uris = data.uris();
        if uris.is_empty() {
            return;
        }

        let mut entries: Vec<String> = Vec::with_capacity(uris.len());
        for uri in uris.iter() {
            let s: &str = uri.as_str();
            let path = match s.strip_prefix("file://") {
                Some(p) => percent_decode(p),
                None => continue,
            };
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            // Cap por archivo: 64 MiB. Evita base64-encodear videos enormes
            // y colgar el GTK main thread.
            if bytes.len() > 64 * 1024 * 1024 {
                continue;
            }
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_string());
            let mime = guess_mime(&name);
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            entries.push(format!(
                "{{name:{n},type:{t},bytes:'{b}'}}",
                n = json_str(&name),
                t = json_str(&mime),
                b = b64
            ));
        }
        if entries.is_empty() {
            return;
        }

        let js = format!(
            r#"(function(){{
  try {{
    var files=[{arr}];
    function b64(s){{var bin=atob(s);var u=new Uint8Array(bin.length);for(var i=0;i<bin.length;i++)u[i]=bin.charCodeAt(i);return u;}}
    var dt=new DataTransfer();
    files.forEach(function(f){{
      try {{ dt.items.add(new File([b64(f.bytes)], f.name, {{type:f.type}})); }} catch(e) {{}}
    }});
    var x={x}, y={y};
    var el=document.elementFromPoint(x,y)||document.body;
    function fire(t){{
      try {{
        el.dispatchEvent(new DragEvent(t,{{bubbles:true,cancelable:true,dataTransfer:dt,clientX:x,clientY:y}}));
      }} catch(e) {{
        var ev=document.createEvent('Event'); ev.initEvent(t,true,true); ev.dataTransfer=dt; ev.clientX=x; ev.clientY=y;
        el.dispatchEvent(ev);
      }}
    }}
    fire('dragenter'); fire('dragover'); fire('drop');
  }} catch(e) {{}}
}})();"#,
            arr = entries.join(","),
            x = x,
            y = y
        );

        w.evaluate_javascript(&js, None, None, None::<&gio::Cancellable>, |_| {});
    },
    );
}

#[cfg(target_os = "linux")]
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(target_os = "linux")]
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(h) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(h);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(target_os = "linux")]
fn guess_mime(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "txt" | "md" => "text/plain",
        "json" => "application/json",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}
