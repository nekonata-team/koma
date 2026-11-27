(() => {
  // src/web-dom/entry.ts
  window.__komaWebDOM = window.__komaWebDOM || (() => {
    const komaRefStore = {
      elements: window.__komaRefs || new Map,
      nextRef: window.__komaNextRef || 1
    };
    window.__komaRefs = komaRefStore.elements;
    window.__komaNextRef = komaRefStore.nextRef;
    function komaNormalize(value) {
      return String(value || "").trim().replace(/\s+/g, " ");
    }
    function komaIsVisible(el) {
      if (!el || el.nodeType !== 1 || !el.isConnected)
        return false;
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0)
        return false;
      const style = window.getComputedStyle(el);
      if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse" || style.opacity === "0")
        return false;
      return true;
    }
    function komaElementForRef(ref) {
      const entry = komaRefStore.elements.get(ref);
      if (!entry)
        return null;
      if ("deref" in entry)
        return entry.deref() ?? null;
      return entry;
    }
    function komaRefFor(el) {
      if (!el || !el.isConnected)
        return null;
      const existing = el.getAttribute("data-koma-ref");
      if (existing && /^@e\d+$/.test(existing)) {
        const mapped = komaElementForRef(existing);
        if (mapped === el)
          return existing;
        if (!mapped || !mapped.isConnected) {
          komaRefStore.elements.set(existing, new WeakRef(el));
          return existing;
        }
      }
      let ref;
      do {
        ref = "@e" + komaRefStore.nextRef++;
      } while (komaRefStore.elements.has(ref));
      window.__komaNextRef = komaRefStore.nextRef;
      komaRefStore.elements.set(ref, new WeakRef(el));
      el.setAttribute("data-koma-ref", ref);
      return ref;
    }
    function komaResolveRef(target) {
      if (!/^@e\d+$/.test(target))
        return null;
      const mapped = komaElementForRef(target);
      if (mapped?.isConnected)
        return mapped;
      const fallback = Array.from(document.querySelectorAll("[data-koma-ref]")).find((el) => el.getAttribute("data-koma-ref") === target) ?? null;
      if (fallback) {
        komaRefStore.elements.set(target, new WeakRef(fallback));
        return fallback;
      }
      komaRefStore.elements.delete(target);
      return null;
    }
    function komaResolveTarget(target) {
      if (target.startsWith("@"))
        return komaResolveRef(target);
      return document.querySelector(target);
    }
    function komaRole(el) {
      const explicit = komaNormalize(el.getAttribute("role")).toLowerCase();
      if (explicit)
        return explicit.split(/\s+/)[0];
      const tag = el.tagName.toLowerCase();
      if (tag === "a" && el.hasAttribute("href"))
        return "link";
      if (tag === "button" || tag === "summary")
        return "button";
      if (tag === "textarea" || el instanceof HTMLElement && el.isContentEditable)
        return "textbox";
      if (tag === "select")
        return "combobox";
      if (tag === "option")
        return "option";
      if (/^h[1-6]$/.test(tag))
        return "heading";
      if (["ul", "ol"].includes(tag))
        return "list";
      if (tag === "li")
        return "listitem";
      if (tag === "nav")
        return "navigation";
      if (tag === "main")
        return "main";
      if (tag === "aside")
        return "complementary";
      if (tag === "header")
        return "banner";
      if (tag === "footer")
        return "contentinfo";
      if (tag === "form")
        return "form";
      if (tag === "img")
        return "img";
      if (tag === "table")
        return "table";
      if (tag === "tr")
        return "row";
      if (tag === "th")
        return "columnheader";
      if (tag === "td")
        return "cell";
      if (tag === "dialog")
        return "dialog";
      if (tag === "input") {
        const type = (el.getAttribute("type") || "text").toLowerCase();
        if (type === "checkbox")
          return "checkbox";
        if (type === "radio")
          return "radio";
        if (["submit", "reset", "button", "image"].includes(type))
          return "button";
        if (!["hidden", "file", "range", "color"].includes(type))
          return "textbox";
      }
      return tag;
    }
    function komaText(el) {
      const innerText = el instanceof HTMLElement ? el.innerText : "";
      return komaNormalize(innerText || el.textContent || "");
    }
    function komaAccessibleName(el) {
      const labelledBy = el.getAttribute("aria-labelledby");
      if (labelledBy) {
        const labelledText = labelledBy.split(/\s+/).map((id) => document.getElementById(id)).filter((node) => node !== null).map((node) => komaText(node)).filter(Boolean).join(" ");
        if (labelledText)
          return labelledText;
      }
      const ariaLabel = komaNormalize(el.getAttribute("aria-label"));
      if (ariaLabel)
        return ariaLabel;
      const labels = el.labels;
      if (labels?.length) {
        const labelText = Array.from(labels).map((label) => komaText(label)).filter(Boolean).join(" ");
        if (labelText)
          return labelText;
      }
      const alt = komaNormalize(el.getAttribute("alt"));
      if (alt)
        return alt;
      const placeholder = komaNormalize(el.getAttribute("placeholder"));
      if (placeholder)
        return placeholder;
      const type = (el.getAttribute("type") || "").toLowerCase();
      if (el.tagName.toLowerCase() === "input" && "value" in el && typeof el.value === "string" && ["button", "submit", "reset"].includes(type)) {
        const value = komaNormalize(el.value);
        if (value)
          return value;
      }
      const title = komaNormalize(el.getAttribute("title"));
      if (title)
        return title;
      return komaText(el);
    }
    function komaValue(el) {
      if (!el)
        return null;
      if ("value" in el && typeof el.value === "string")
        return el.value;
      if (el instanceof HTMLElement && el.isContentEditable || ["textbox", "searchbox", "combobox"].includes(komaRole(el))) {
        const innerText = el instanceof HTMLElement ? el.innerText : "";
        const text = innerText || el.textContent || "";
        return text === `
` ? "" : text;
      }
      return null;
    }
    function komaChecked(el) {
      if (!el)
        return null;
      const type = (el.getAttribute("type") || "").toLowerCase();
      if ("checked" in el && typeof el.checked === "boolean" && ["checkbox", "radio"].includes(type)) {
        return !!el.checked;
      }
      const ariaChecked = el.getAttribute("aria-checked");
      if (ariaChecked === "true")
        return true;
      if (ariaChecked === "false")
        return false;
      return null;
    }
    function komaDisabled(el) {
      if (!el)
        return null;
      if (el.matches(":disabled"))
        return true;
      const ariaDisabled = el.getAttribute("aria-disabled");
      if (ariaDisabled === "true")
        return true;
      if (ariaDisabled === "false")
        return false;
      const nativeControl = ["button", "fieldset", "input", "optgroup", "option", "select", "textarea"].includes(el.tagName.toLowerCase());
      return nativeControl ? !!(("disabled" in el) && el.disabled) : null;
    }
    function komaSelected(el) {
      if (!el)
        return null;
      if (el.tagName.toLowerCase() === "option" && "selected" in el)
        return !!el.selected;
      const ariaSelected = el.getAttribute("aria-selected");
      if (ariaSelected === "true")
        return true;
      if (ariaSelected === "false")
        return false;
      return null;
    }
    function komaExpanded(el) {
      if (!el)
        return null;
      if (el.tagName.toLowerCase() === "details" && "open" in el)
        return !!el.open;
      const ariaExpanded = el.getAttribute("aria-expanded");
      if (ariaExpanded === "true")
        return true;
      if (ariaExpanded === "false")
        return false;
      return null;
    }
    function komaSnapshotStates(el) {
      const states = [];
      const checked = komaChecked(el);
      if (checked !== null)
        states.push(checked ? "checked" : "unchecked");
      const disabled = komaDisabled(el);
      if (disabled === true)
        states.push("disabled");
      const selected = komaSelected(el);
      if (selected === true)
        states.push("selected");
      const expanded = komaExpanded(el);
      if (expanded !== null)
        states.push(expanded ? "expanded" : "collapsed");
      return states;
    }
    function komaSnapshotValue(el) {
      if (el.tagName.toLowerCase() === "input" && (el.getAttribute("type") || "text").toLowerCase() === "password")
        return null;
      return ["textbox", "searchbox", "combobox"].includes(komaRole(el)) ? komaValue(el) : null;
    }
    function komaSnapshotContextText(el, interactiveSelector) {
      if (!["alertdialog", "dialog", "listbox", "menu", "menubar"].includes(komaRole(el)))
        return [];
      const texts = [];
      let visited = 0;
      let characterCount = 0;
      let truncated = false;
      const visit = (node) => {
        if (visited++ >= 50)
          return;
        if (!komaIsVisible(node) || node.hasAttribute("aria-live") || ["status", "alert", "log"].includes(komaNormalize(node.getAttribute("role")).toLowerCase()))
          return;
        if (node !== el && node.matches(interactiveSelector))
          return;
        const rect = node.getBoundingClientRect();
        if (rect.bottom <= 0 || rect.right <= 0 || rect.top >= window.innerHeight || rect.left >= window.innerWidth)
          return;
        for (const child of Array.from(node.childNodes)) {
          if (child.nodeType === Node.TEXT_NODE) {
            const text = komaNormalize(child.textContent);
            if (text && characterCount < 80) {
              const remaining = 80 - characterCount;
              if (text.length > remaining)
                truncated = true;
              texts.push(text.slice(0, remaining));
              characterCount += text.length + 1;
            }
          }
        }
        for (const child of Array.from(node.children)) {
          if (!child.matches(interactiveSelector))
            visit(child);
        }
      };
      visit(el);
      const text = komaNormalize(texts.join(" "));
      return text ? [truncated ? text.slice(0, 77) + "..." : text] : [];
    }
    function komaSnapshotContextRole(el) {
      return [
        "alertdialog",
        "banner",
        "complementary",
        "contentinfo",
        "dialog",
        "form",
        "grid",
        "list",
        "listbox",
        "main",
        "menu",
        "menubar",
        "navigation",
        "region",
        "toolbar",
        "tree",
        "treegrid"
      ].includes(komaRole(el));
    }
    const snapshotSelectors = [
      "a[href]", "button", "input", "select", "textarea",
      '[role="button"]', '[role="link"]', '[role="checkbox"]', '[role="menuitem"]',
      '[role="tab"]', '[role="switch"]', '[tabindex]:not([tabindex="-1"])',
      "summary", '[contenteditable="true"]'
    ];
    function komaSnapshot(within = null, selectors = snapshotSelectors, interactiveOnly = true) {
      let scope = document;
      if (within !== null) {
        try {
          if (within.startsWith("@")) {
            const target = komaResolveTarget(within);
            if (!target)
              return { ok: false, error: "Scope not found: " + within };
            scope = target;
          } else {
            const scopes = Array.from(document.querySelectorAll(within));
            if (scopes.length === 0)
              return { ok: false, error: "Scope not found: " + within };
            if (scopes.length > 1)
              return { ok: false, error: "Scope matched multiple elements: " + within };
            scope = scopes[0];
          }
        } catch {
          return { ok: false, error: "Invalid scope selector: " + within };
        }
      }
      const interactiveSelector = selectors.join(",");
      const interactiveElements = scope instanceof Element ? [scope, ...Array.from(scope.querySelectorAll(interactiveSelector))] : Array.from(scope.querySelectorAll(interactiveSelector));
      const elements = interactiveOnly ? new Set(interactiveElements.filter((el) => komaIsVisible(el) && komaSnapshotEligible(el, selectors) && el.matches(interactiveSelector))) : new Set((scope instanceof Element ? [scope, ...Array.from(scope.querySelectorAll("*"))] : Array.from(scope.querySelectorAll("*"))).filter((el) => komaIsVisible(el) && komaSnapshotEligible(el, selectors)));
      if (interactiveOnly) {
        for (const el of interactiveElements) {
          if (!elements.has(el) || !komaIsVisible(el))
            continue;
          let parent = el.parentElement;
          while (parent && (!(scope instanceof Element) || parent !== scope.parentElement)) {
            if (komaSnapshotContextRole(parent) && komaIsVisible(parent) && komaSnapshotEligible(parent, selectors)) {
              elements.add(parent);
            }
            if (parent === scope)
              break;
            parent = parent.parentElement;
          }
        }
      }
      const elementSet = new Set(elements);
      const lines = [];
      const orderedElements = Array.from(elements).sort((a, b) => {
        const position = a.compareDocumentPosition(b);
        return position & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : position & Node.DOCUMENT_POSITION_PRECEDING ? 1 : 0;
      });
      for (const el of orderedElements) {
        const ref = komaRefFor(el);
        const role = komaRole(el);
        const level = komaSnapshotLevel(el);
        const name = komaSnapshotName(el, role);
        const truncatedName = name.length > 80 ? name.slice(0, 77) + "..." : name;
        let depth = 0;
        let parent = el.parentElement;
        while (parent) {
          if (elementSet.has(parent))
            depth++;
          parent = parent.parentElement;
        }
        let line = "  ".repeat(depth) + ref + " [" + role;
        if (level !== null)
          line += ":level=" + level;
        line += "]";
        if (truncatedName)
          line += " " + JSON.stringify(truncatedName);
        const value = komaSnapshotValue(el);
        if (value !== null)
          line += " [value=" + JSON.stringify(value.length > 80 ? value.slice(0, 77) + "..." : value) + "]";
        const states = komaSnapshotStates(el);
        if (states.length)
          line += " [" + states.join(",") + "]";
        lines.push(line);
        if (komaSnapshotContextRole(el)) {
          for (const text of komaSnapshotContextText(el, interactiveSelector)) {
            lines.push("  ".repeat(depth + 1) + "text " + JSON.stringify(text));
          }
        }
      }
      return { ok: true, snapshot: lines.join(`
`) };
    }
    function komaSnapshotEligible(el, selectors) {
      const tag = el.tagName.toLowerCase();
      const explicitRole = komaNormalize(el.getAttribute("role")).toLowerCase();
      if (el.closest('[aria-hidden="true"]') || ["none", "presentation"].includes(explicitRole)) {
        return false;
      }
      if (el.hasAttribute("role") || el.hasAttribute("aria-label") || el.hasAttribute("aria-labelledby")) {
        return true;
      }
      if (el.matches(selectors.join(",")))
        return true;
      return [
        "a",
        "button",
        "form",
        "footer",
        "header",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "img",
        "li",
        "main",
        "nav",
        "ol",
        "option",
        "section",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "tr",
        "ul",
        "textarea",
        "select",
        "dialog",
        "details",
        "summary"
      ].includes(tag);
    }
    function komaSnapshotName(el, role) {
      const hasExplicitName = el.hasAttribute("aria-label") || el.hasAttribute("aria-labelledby");
      const namedRoles = [
        "button",
        "cell",
        "checkbox",
        "columnheader",
        "combobox",
        "dialog",
        "heading",
        "img",
        "link",
        "listitem",
        "menuitem",
        "option",
        "radio",
        "row",
        "switch",
        "tab",
        "textbox"
      ];
      if (!hasExplicitName && !namedRoles.includes(role))
        return "";
      return komaAccessibleName(el);
    }
    function komaSnapshotLevel(el) {
      const tag = el.tagName.toLowerCase();
      if (/^h[1-6]$/.test(tag))
        return Number(tag.slice(1));
      return null;
    }
    function komaDispatchInput(el, value, isDelete = false) {
      el.dispatchEvent(new InputEvent("input", {
        bubbles: true,
        cancelable: true,
        inputType: isDelete ? "deleteContentBackward" : "insertText",
        data: isDelete ? null : value
      }));
      el.dispatchEvent(new Event("change", { bubbles: true }));
    }
    function komaSetValue(el, value) {
      if (!el)
        return false;
      if ("value" in el && typeof el.value === "string") {
        const descriptor = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), "value");
        descriptor?.set?.call(el, value);
        el.value = value;
        komaDispatchInput(el, value);
        return true;
      }
      if (el instanceof HTMLElement && el.isContentEditable || ["textbox", "searchbox"].includes(komaRole(el))) {
        if ("focus" in el && typeof el.focus === "function")
          el.focus();
        try {
          window.getSelection()?.selectAllChildren(el);
        } catch (_) {}
        const ok = value ? document.execCommand("insertText", false, value) : document.execCommand("delete", false);
        if (!ok || !value)
          el.textContent = value;
        komaDispatchInput(el, value, !value);
        return true;
      }
      return false;
    }
    function komaInspect(el, fields) {
      const info = {
        ref: komaRefFor(el),
        visible: komaIsVisible(el)
      };
      if (fields.includes("tag"))
        info.tag = el.tagName.toLowerCase();
      if (fields.includes("role"))
        info.role = komaRole(el);
      if (fields.includes("name"))
        info.name = komaAccessibleName(el);
      if (fields.includes("text"))
        info.text = komaText(el);
      if (fields.includes("value")) {
        const value = komaValue(el);
        if (value !== null)
          info.value = value;
      }
      if (fields.includes("checked")) {
        const checked = komaChecked(el);
        if (checked !== null)
          info.checked = checked;
      }
      if (fields.includes("disabled")) {
        const disabled = komaDisabled(el);
        if (disabled !== null)
          info.disabled = disabled;
      }
      if (fields.includes("selected")) {
        const selected = komaSelected(el);
        if (selected !== null)
          info.selected = selected;
      }
      if (fields.includes("expanded")) {
        const expanded = komaExpanded(el);
        if (expanded !== null)
          info.expanded = expanded;
      }
      const attributes = {};
      fields.forEach((field) => {
        let attribute = null;
        if (field === "class")
          attribute = "class";
        if (field.startsWith("attr:"))
          attribute = field.slice(5);
        if (attribute) {
          const value = el.getAttribute(attribute);
          if (value !== null)
            attributes[attribute] = value;
        }
      });
      if (Object.keys(attributes).length)
        info.attributes = attributes;
      return info;
    }
    return {
      komaNormalize,
      komaIsVisible,
      komaRefFor,
      komaResolveRef,
      komaResolveTarget,
      komaRole,
      komaText,
      komaAccessibleName,
      komaValue,
      komaChecked,
      komaDisabled,
      komaSelected,
      komaExpanded,
      komaSnapshotStates,
      komaSnapshotValue,
      komaSnapshotContextText,
      komaSnapshotContextRole,
      komaSnapshot,
      komaSnapshotEligible,
      komaSnapshotName,
      komaSnapshotLevel,
      komaDispatchInput,
      komaSetValue,
      komaInspect
    };
  })();
})();
