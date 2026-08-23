/* mdview — viewer runtime.
 *
 * Runs inside the webview and owns everything that must happen after the HTML
 * lands: theme resolution, Mermaid rendering, scroll restoration, zoom, and
 * forwarding keyboard shortcuts to the host process.
 */
(function () {
  "use strict";

  var root = document.documentElement;
  var docKey = "mdview:" + (root.getAttribute("data-doc") || location.pathname);

  /* ------------------------------------------------------------------ ipc */

  function send(type, value) {
    try {
      window.ipc.postMessage(value === undefined ? type : type + ":" + value);
    } catch (err) {
      /* Running outside the host (e.g. a plain browser); shortcuts are inert. */
    }
  }

  function store(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch (err) {
      /* Storage can be unavailable; every caller has a working default. */
    }
  }

  function load(key, fallback) {
    try {
      var value = localStorage.getItem(key);
      return value === null ? fallback : value;
    } catch (err) {
      return fallback;
    }
  }

  /* ---------------------------------------------------------------- theme */

  var media = window.matchMedia ? window.matchMedia("(prefers-color-scheme: dark)") : null;

  function preference() {
    return root.getAttribute("data-theme-preference") || "auto";
  }

  function resolveTheme() {
    var pref = preference();
    if (pref === "light" || pref === "dark") return pref;
    return media && media.matches ? "dark" : "light";
  }

  function applyTheme() {
    var theme = resolveTheme();
    if (root.getAttribute("data-theme") !== theme) {
      root.setAttribute("data-theme", theme);
      renderMermaid();
    }
    return theme;
  }

  if (media) {
    var onSchemeChange = function () {
      if (preference() === "auto") applyTheme();
    };
    if (media.addEventListener) media.addEventListener("change", onSchemeChange);
    else if (media.addListener) media.addListener(onSchemeChange);
  }

  /* ----------------------------------------------------------------- zoom */

  var ZOOM_STEPS = [0.75, 0.85, 1, 1.15, 1.3, 1.5, 1.75, 2];
  var zoomIndex = clampZoom(parseInt(load("mdview:zoom", "2"), 10));

  function clampZoom(index) {
    if (isNaN(index)) return 2;
    return Math.min(ZOOM_STEPS.length - 1, Math.max(0, index));
  }

  function applyZoom(announce) {
    var scale = ZOOM_STEPS[zoomIndex];
    root.style.fontSize = scale * 16 + "px";
    store("mdview:zoom", String(zoomIndex));
    if (announce) toast(Math.round(scale * 100) + "%");
  }

  function zoom(delta) {
    var next = delta === 0 ? 2 : clampZoom(zoomIndex + delta);
    if (next === zoomIndex && delta !== 0) return;
    zoomIndex = next;
    applyZoom(true);
  }

  /* ---------------------------------------------------------------- toast */

  var toastEl = null;
  var toastTimer = null;

  function toast(message) {
    if (!toastEl) {
      toastEl = document.createElement("div");
      toastEl.className = "md-toast";
      document.body.appendChild(toastEl);
    }
    toastEl.textContent = message;
    toastEl.setAttribute("data-visible", "true");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () {
      toastEl.removeAttribute("data-visible");
    }, 1200);
  }

  /* --------------------------------------------------------------- tables */

  function wrapTables() {
    var tables = document.querySelectorAll(".md-body > table");
    for (var i = 0; i < tables.length; i++) {
      var wrapper = document.createElement("div");
      wrapper.className = "md-table-scroll";
      tables[i].parentNode.insertBefore(wrapper, tables[i]);
      wrapper.appendChild(tables[i]);
    }
  }

  /* -------------------------------------------------------------- mermaid */

  var mermaidSources = null;
  var mermaidRunId = 0;

  function collectMermaid() {
    var nodes = document.querySelectorAll("pre.mermaid");
    mermaidSources = [];
    for (var i = 0; i < nodes.length; i++) {
      mermaidSources.push({ el: nodes[i], source: nodes[i].textContent });
    }
    return mermaidSources.length > 0;
  }

  function renderMermaid() {
    if (!window.mermaid || !mermaidSources || mermaidSources.length === 0) return;

    var runId = ++mermaidRunId;
    window.mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: resolveTheme() === "dark" ? "dark" : "default",
      fontFamily: getComputedStyle(root).getPropertyValue("--font-body") || "sans-serif",
    });

    var pending = mermaidSources.map(function (entry, index) {
      return window.mermaid
        .render("mdview-mermaid-" + runId + "-" + index, entry.source)
        .then(function (result) {
          if (runId !== mermaidRunId) return;
          entry.el.innerHTML = result.svg;
          if (typeof result.bindFunctions === "function") result.bindFunctions(entry.el);
          entry.el.removeAttribute("data-failed");
          entry.el.setAttribute("data-processed", "true");
        })
        .catch(function (err) {
          if (runId !== mermaidRunId) return;
          entry.el.textContent = "Mermaid error: " + (err && err.message ? err.message : err) +
            "\n\n" + entry.source;
          entry.el.setAttribute("data-failed", "true");
          entry.el.setAttribute("data-processed", "true");
        });
    });

    return Promise.all(pending);
  }

  /* --------------------------------------------------------------- scroll */

  function saveScroll() {
    try {
      sessionStorage.setItem(docKey, String(window.scrollY));
    } catch (err) {
      /* Nothing to restore next time; harmless. */
    }
  }

  function restoreScroll() {
    if (location.hash) return;
    try {
      var saved = sessionStorage.getItem(docKey);
      if (saved !== null) window.scrollTo(0, parseInt(saved, 10) || 0);
    } catch (err) {
      /* Start at the top. */
    }
  }

  window.addEventListener("beforeunload", saveScroll);
  window.addEventListener("pagehide", saveScroll);
  setInterval(saveScroll, 1000);

  /* ------------------------------------------------------------ shortcuts */

  document.addEventListener("keydown", function (event) {
    var mod = event.metaKey || event.ctrlKey;

    if (mod) {
      switch (event.key) {
        case "o":
          event.preventDefault();
          return send("open");
        case "r":
          event.preventDefault();
          return send("reload");
        case "q":
          event.preventDefault();
          return send("quit");
        case "w":
          event.preventDefault();
          return send("quit");
        case "d":
          event.preventDefault();
          return send("toggle-theme");
        case "p":
          event.preventDefault();
          return window.print();
        case "=":
        case "+":
          event.preventDefault();
          return zoom(1);
        case "-":
          event.preventDefault();
          return zoom(-1);
        case "0":
          event.preventDefault();
          return zoom(0);
        default:
          return;
      }
    }

    if (event.altKey || event.ctrlKey || event.metaKey) return;
    if (/^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName)) return;

    switch (event.key) {
      case "F5":
        event.preventDefault();
        return send("reload");
      case "g":
        event.preventDefault();
        return window.scrollTo({ top: 0, behavior: "smooth" });
      case "G":
        event.preventDefault();
        return window.scrollTo({ top: document.body.scrollHeight, behavior: "smooth" });
      case "j":
        event.preventDefault();
        return window.scrollBy({ top: 80, behavior: "smooth" });
      case "k":
        event.preventDefault();
        return window.scrollBy({ top: -80, behavior: "smooth" });
      default:
        return;
    }
  });

  /* Ctrl/Cmd + wheel zoom, matching the keyboard steps. */
  window.addEventListener(
    "wheel",
    function (event) {
      if (!(event.metaKey || event.ctrlKey)) return;
      event.preventDefault();
      zoom(event.deltaY < 0 ? 1 : -1);
    },
    { passive: false }
  );

  /* The host owns navigation; never let a document replace itself. */
  window.addEventListener("dragover", function (e) { e.preventDefault(); });
  window.addEventListener("drop", function (e) { e.preventDefault(); });

  /* ----------------------------------------------------------------- boot */

  applyTheme();
  applyZoom(false);
  wrapTables();

  if (collectMermaid()) {
    var done = renderMermaid();
    if (done && done.then) done.then(restoreScroll);
    else restoreScroll();
  } else {
    restoreScroll();
  }

  window.mdview = {
    applyTheme: applyTheme,
    renderMermaid: renderMermaid,
    toast: toast,
    zoom: zoom,
  };
})();
