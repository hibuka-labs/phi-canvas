/*
 * phi-canvas compare helper: picker navigation and the wipe slider.
 *
 * Deliberately separate from helper.js — a compare page is history the user
 * is looking at, so it neither polls for new screens nor posts clicks. There
 * is no session token on this page and nothing here can reach /events.
 */
(function () {
  "use strict";

  function el(id) {
    return document.getElementById(id);
  }

  function currentMode() {
    return /\/slide\/?$/.test(window.location.pathname) ? "slide" : "split";
  }

  function navigate() {
    var a = el("pick-a");
    var b = el("pick-b");
    if (!a || !b) return;
    // Path segments, never a query string: the server rejects every `?…`
    // outright, which is how the session token stays out of URLs.
    window.location.pathname = "/compare/" + a.value + "/" + b.value + "/" + currentMode();
  }

  ["pick-a", "pick-b"].forEach(function (id) {
    var picker = el(id);
    if (picker) {
      picker.addEventListener("change", navigate);
    }
  });

  var range = el("wipe-range");
  var stack = el("wipe-stack");
  if (range && stack) {
    var apply = function () {
      stack.style.setProperty("--wipe", range.value + "%");
    };
    range.addEventListener("input", apply);
    apply();
  }
})();
