(function () {
  if (window.__KOLIBRI_BAR__) return;
  window.__KOLIBRI_BAR__ = true;

  // Spoof navigator props para que sitios (WhatsApp Web, etc) no nos crean Mac/Safari/iOS.
  // initialization_script corre antes que los scripts del sitio.
  try {
    var def = function (prop, value) {
      try {
        Object.defineProperty(navigator, prop, { get: function () { return value; }, configurable: true });
      } catch (e) {}
    };
    def('platform', 'Linux x86_64');
    def('vendor', 'Google Inc.');
    def('vendorSub', '');
    def('product', 'Gecko');
    def('productSub', '20030107');
    def('appName', 'Netscape');
    def('appCodeName', 'Mozilla');
    def('appVersion', '5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36');
    def('oscpu', undefined);
    def('maxTouchPoints', 0);
  } catch (e) { console.warn('[Kolibri] navigator spoof falló:', e); }

  var BAR_H = 44;
  var POLL_MS = 1500;

  function ready(cb) {
    if (window.__TAURI__ && document.body) cb();
    else setTimeout(function () { ready(cb); }, 30);
  }

  function el(tag, attrs, children) {
    var e = document.createElement(tag);
    if (attrs) {
      for (var k in attrs) {
        if (k === 'style') Object.assign(e.style, attrs[k]);
        else if (k === 'onclick') e.onclick = attrs[k];
        else if (k === 'class') e.className = attrs[k];
        else if (k === 'title') e.title = attrs[k];
        else e.setAttribute(k, attrs[k]);
      }
    }
    if (children) {
      for (var i = 0; i < children.length; i++) {
        var c = children[i];
        e.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
      }
    }
    return e;
  }

  var CSS =
    "#__klb_bar { position: fixed !important; top:0 !important; left:0 !important; right:0 !important;" +
    "  height:" + BAR_H + "px !important; background:#222 !important; color:#eee !important;" +
    "  z-index:2147483647 !important; display:flex !important; align-items:center !important;" +
    "  padding:0 6px !important; gap:4px !important; font-family:system-ui,sans-serif !important;" +
    "  border-bottom:1px solid #2c2c2c !important; box-sizing:border-box !important;" +
    "  user-select:none !important; pointer-events:auto !important; }" +
    "#__klb_bar * { box-sizing:border-box !important; }" +
    ".__klb_btn { background:transparent; border:none; color:#bbb; cursor:pointer;" +
    "  width:38px; height:34px; border-radius:6px; font-size:14px;" +
    "  display:grid; place-items:center; }" +
    ".__klb_btn:hover { background:#333; color:#fff; }" +
    ".__klb_close_w:hover { background:#c00 !important; color:#fff !important; }" +
    ".__klb_add { background:#2c2c2c; border:1px solid #3a3a3a; color:#ccc;" +
    "  width:34px; height:34px; border-radius:8px; font-size:18px; cursor:pointer;" +
    "  display:grid; place-items:center; flex-shrink:0; }" +
    ".__klb_add:hover { background:#383838; color:#fff; }" +
    ".__klb_tabs { display:flex; gap:2px; flex:0 1 auto; min-width:0;" +
    "  overflow-x:auto; height:100%; align-items:center; }" +
    ".__klb_tabs::-webkit-scrollbar { display:none; }" +
    ".__klb_tab { display:flex; align-items:center; gap:8px; height:34px;" +
    "  padding:0 8px 0 4px; background:#2a2a2a; border:1px solid transparent;" +
    "  border-radius:8px; color:#ccc; cursor:pointer; font-size:12px;" +
    "  max-width:180px; flex-shrink:0; }" +
    ".__klb_tab:hover { background:#333; }" +
    ".__klb_tab.active { background:#383838; border-color:#4a4a4a; color:#fff; }" +
    ".__klb_icon { width:26px !important; height:26px !important; border-radius:6px !important;" +
    "  display:grid !important; place-items:center !important; color:#fff !important;" +
    "  font-weight:700 !important; font-size:12px !important; flex-shrink:0 !important;" +
    "  font-family:system-ui,sans-serif !important; line-height:1 !important; }" +
    ".__klb_name { white-space:nowrap; overflow:hidden; text-overflow:ellipsis;" +
    "  flex:1; min-width:0; }" +
    ".__klb_close { color:#777; font-size:14px; padding:2px 4px; border-radius:4px;" +
    "  cursor:pointer; line-height:1; }" +
    ".__klb_close:hover { background:#4a2222; color:#f88; }" +
    ".__klb_spacer { flex:1; min-width:12px; height:100%; }" +
    ".__klb_ctrls { display:flex; height:100%; align-items:center; flex-shrink:0; }" +
    "html, body { background: #1a1a1a !important; }" +
    "body { margin-top:" + BAR_H + "px !important; }";

  var services = [];
  var activeId = null;
  var bar = null;

  ready(function () {
    var T = window.__TAURI__;
    var invoke = T.core.invoke;

    var style = document.createElement('style');
    style.textContent = CSS;
    document.head.appendChild(style);

    function makeBar() {
      var b = el('div', { id: '__klb_bar' });
      b.setAttribute('data-tauri-drag-region', 'true');
      b.addEventListener('mousedown', function (ev) {
        if (ev.button !== 0) return;
        var t = ev.target;
        if (t && t.closest && t.closest('button, .__klb_close, [data-no-drag]')) return;
        if (ev.detail === 2) {
          invoke('window_toggle_maximize');
          return;
        }
        invoke('plugin:window|start_dragging').catch(function (e) { console.error('drag', e); });
      });
      return b;
    }

    function ensureStyleAttached() {
      if (!document.head.contains(style)) document.head.appendChild(style);
    }
    function ensureBarAttached() {
      if (!document.body) return;
      if (!document.body.contains(bar)) {
        bar = makeBar();
        document.body.appendChild(bar);
        render();
      }
    }

    bar = makeBar();
    document.body.appendChild(bar);

    // Re-inject si el sitio (Gmail/SPAs) borra body.
    var mo = new MutationObserver(function () {
      ensureStyleAttached();
      ensureBarAttached();
    });
    mo.observe(document.documentElement, { childList: true, subtree: true });

    function render() {
      while (bar.firstChild) bar.removeChild(bar.firstChild);
      var addBtn = el('button', {
        class: '__klb_add',
        title: 'Agregar servicio',
        onclick: function () { invoke('open_add_dialog'); }
      }, ['＋']);
      bar.appendChild(addBtn);

      var tabs = el('div', { class: '__klb_tabs' });
      tabs.setAttribute('data-tauri-drag-region', 'true');
      services.forEach(function (s) {
        var initial = (s.icon && s.icon.length) ? s.icon.charAt(0) : s.name.charAt(0).toUpperCase();
        var tab = el('button', {
          class: '__klb_tab' + (s.id === activeId ? ' active' : ''),
          title: s.name,
          onclick: function () { invoke('switch_service', { id: s.id }); }
        });
        var iconSpan = el('span', { class: '__klb_icon' }, [initial]);
        iconSpan.style.setProperty('background', s.color || '#444', 'important');
        tab.appendChild(iconSpan);
        tab.appendChild(el('span', { class: '__klb_name' }, [s.name]));
        var closeBtn = el('span', { class: '__klb_close', title: 'Eliminar' }, ['×']);
        closeBtn.onclick = function (ev) {
          ev.stopPropagation();
          if (confirm('¿Eliminar ' + s.name + '?')) invoke('remove_service', { id: s.id });
        };
        tab.appendChild(closeBtn);
        tabs.appendChild(tab);
      });
      bar.appendChild(tabs);

      var sp = el('div', { class: '__klb_spacer' });
      sp.setAttribute('data-tauri-drag-region', 'true');
      bar.appendChild(sp);

      var ctrls = el('div', { class: '__klb_ctrls' });
      ctrls.appendChild(el('button', { class: '__klb_btn', title: 'Configuración', onclick: function () { invoke('open_settings'); } }, ['⚙']));
      ctrls.appendChild(el('button', { class: '__klb_btn', title: 'Minimizar', onclick: function () { invoke('window_minimize'); } }, ['─']));
      ctrls.appendChild(el('button', { class: '__klb_btn', title: 'Maximizar', onclick: function () { invoke('window_toggle_maximize'); } }, ['▢']));
      ctrls.appendChild(el('button', { class: '__klb_btn __klb_close_w', title: 'Cerrar', onclick: function () { invoke('window_close'); } }, ['×']));
      bar.appendChild(ctrls);
    }

    function refresh() {
      Promise.all([invoke('list_services'), invoke('get_active_service')])
        .then(function (r) { services = r[0] || []; activeId = r[1]; render(); })
        .catch(function (e) {
          console.error('[Kolibri bar] refresh error:', e);
          render(); // dibuja al menos los controles
        });
    }

    render(); // pinta vacío inmediatamente
    refresh();
    if (T.event && T.event.listen) {
      T.event.listen('kolibri:services_changed', refresh);
      T.event.listen('kolibri:active_changed', refresh);
    }
    setInterval(refresh, POLL_MS);
  });
})();
