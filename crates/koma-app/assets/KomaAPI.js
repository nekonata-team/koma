(() => {
  var __defProp = Object.defineProperty;
  var __getOwnPropNames = Object.getOwnPropertyNames;
  var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
  var __hasOwnProp = Object.prototype.hasOwnProperty;
  function __accessProp(key) {
    return this[key];
  }
  var __toCommonJS = (from) => {
    var entry = (__moduleCache ??= new WeakMap).get(from), desc;
    if (entry)
      return entry;
    entry = __defProp({}, "__esModule", { value: true });
    if (from && typeof from === "object" || typeof from === "function") {
      for (var key of __getOwnPropNames(from))
        if (!__hasOwnProp.call(entry, key))
          __defProp(entry, key, {
            get: __accessProp.bind(from, key),
            enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable
          });
    }
    __moduleCache.set(from, entry);
    return entry;
  };
  var __moduleCache;

  // src/koma-api/entry.ts
  var exports_entry = {};
  window.koma = Object.freeze({
    komas: Object.freeze({
      async focus(komaId) {
        await komaAPITransport.postMessage({ action: "koma.focus", komaId });
      }
    }),
    toast: Object.freeze({
      async show(message, options) {
        await komaAPITransport.postMessage({ action: "toast.show", message, severity: options?.severity ?? "info" });
      }
    })
  });
})();
