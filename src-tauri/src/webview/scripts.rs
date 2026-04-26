use tauri::{
    webview::{PageLoadEvent, WebviewBuilder},
    Emitter, Manager, Runtime,
};

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
