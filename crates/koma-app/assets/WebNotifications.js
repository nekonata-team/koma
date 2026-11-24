((prefix) => {
  if (window.top !== window || !window.isSecureContext || !/^https?:$/.test(location.protocol)) return;
  const origin = window.origin;
  const emit = console.info.bind(console);
  const stringify = JSON.stringify;
  function notify(title, options) {
    emit(prefix + stringify({ origin, title: title.slice(0, 120), body: (options?.body ?? "").toString().slice(0, 360) }));
  }

  class KomaNotification extends EventTarget {
    static get permission() { return "granted"; }
    static get maxActions() { return 0; }

    static requestPermission(callback) {
      if (typeof callback === "function") queueMicrotask(() => callback("granted"));
      return Promise.resolve("granted");
    }

    constructor(title, options = {}) {
      super();
      if (!arguments.length) throw new TypeError("A notification title is required");
      this.title = String(title);
      this.body = String(options?.body ?? "");
      this.tag = String(options?.tag ?? "");
      this.data = options?.data ?? null;
      notify(this.title, { body: this.body });
      queueMicrotask(() => {
        const event = new Event("show");
        this.dispatchEvent(event);
        this.onshow?.(event);
      });
    }

    close() {
      const event = new Event("close");
      this.dispatchEvent(event);
      this.onclose?.(event);
    }
  }

  Object.defineProperty(window, "Notification", {
    configurable: true,
    writable: true,
    value: KomaNotification,
  });

  const registration = window.ServiceWorkerRegistration?.prototype;
  if (registration) {
    registration.showNotification = async function (title, options) {
      if (!arguments.length) throw new TypeError("A notification title is required");
      notify(String(title), options);
    };
  }
})
