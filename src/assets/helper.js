/*
 * phi-canvas page helper: heartbeat poll + click reporting.
 *
 * The page never sends the session token in a URL — it reads the token from
 * the meta tag (injected at serve time) and posts it as `X-Canvas-Token`.
 * Polling doubles as the heartbeat: three consecutive failures render the
 * "connection lost" state (the server being gone cannot notify anyone).
 */
(function () {
  "use strict";

  var tokenEl = document.querySelector('meta[name="canvas-token"]');
  var seqEl = document.querySelector('meta[name="canvas-seq"]');
  var liveEl = document.querySelector('meta[name="canvas-live"]');
  var token = tokenEl ? tokenEl.getAttribute("content") : "";
  var seq = seqEl ? parseInt(seqEl.getAttribute("content"), 10) || 0 : 0;
  // A page that is not the screen on offer (a historical version, a notice)
  // is for looking: it neither chases the current screen nor reports clicks.
  var live = !liveEl || liveEl.getAttribute("content") !== "0";
  var failures = 0;
  var POLL_MS = 2000;
  var FAIL_LIMIT = 3;

  function el(id) {
    return document.getElementById(id);
  }

  function showConnected() {
    var status = el("conn-status");
    if (status) {
      status.classList.remove("disconnected");
      status.textContent = "已连接";
    }
  }

  function showDisconnected() {
    var status = el("conn-status");
    if (status) {
      status.classList.add("disconnected");
      status.textContent = "连接已断开";
    }
    var bar = el("indicator-text");
    if (bar) {
      bar.textContent = "连接已断开 · 正在重连…";
    }
  }

  function poll() {
    fetch("/api/poll", { cache: "no-store" })
      .then(function (res) {
        if (!res.ok) {
          throw new Error("poll status " + res.status);
        }
        return res.json();
      })
      .then(function (state) {
        failures = 0;
        showConnected();
        if (live && typeof state.seq === "number" && state.seq !== seq) {
          window.location.reload();
        }
      })
      .catch(function () {
        failures += 1;
        if (failures >= FAIL_LIMIT) {
          showDisconnected();
        }
      });
  }

  function post(action, extra) {
    var payload = Object.assign({ type: action, screen: seq }, extra || {});
    fetch("/events", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Canvas-Token": token,
      },
      body: JSON.stringify(payload),
    }).catch(function () {
      /* heartbeat will surface a dead server */
    });
  }

  function optionLabel(node) {
    var h = node.querySelector(".content h3, h3, .card-body h3");
    return h ? h.textContent.trim() : (node.getAttribute("data-choice") || "");
  }

  function refreshIndicator() {
    var bar = el("indicator-text");
    if (!bar) return;
    var container = document.querySelector(".options, .cards");
    var picked = container ? Array.prototype.slice.call(container.querySelectorAll(".selected")) : [];
    if (picked.length === 0) {
      bar.textContent = "点击选择，然后回到终端继续";
      return;
    }
    var label = picked.length === 1 ? optionLabel(picked[0]) : String(picked.length) + " 项";
    bar.innerHTML = "";
    var span = document.createElement("span");
    span.className = "selected-text";
    span.textContent = "已选 " + label;
    bar.appendChild(span);
    bar.appendChild(document.createTextNode(" — 回到终端继续"));
  }

  function toggleSelect(node) {
    var container = node.closest(".options") || node.closest(".cards");
    if (!container) return;
    var multi = container.getAttribute("data-multiselect") === "true";
    if (!multi) {
      container.querySelectorAll(".selected").forEach(function (other) {
        other.classList.remove("selected");
      });
      node.classList.add("selected");
    } else {
      node.classList.toggle("selected");
    }
  }

  document.addEventListener("click", function (event) {
    if (!live) return;
    var target = event.target;
    if (!(target instanceof Element)) return;

    var choice = target.closest("[data-choice]");
    if (choice) {
      toggleSelect(choice);
      post("select", { option_id: choice.getAttribute("data-choice") });
      refreshIndicator();
      return;
    }

    var action = target.closest("[data-action]");
    if (!action) return;
    var name = action.getAttribute("data-action");
    if (name === "skip") {
      post("skip");
      return;
    }
    if (name === "confirm") {
      post("confirm");
    }
  });

  setInterval(poll, POLL_MS);
  poll();
})();
