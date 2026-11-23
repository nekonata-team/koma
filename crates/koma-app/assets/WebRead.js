/* Turndown and turndown-plugin-gfm; includes collapse-whitespace
Copyright (c) 2014 Luc Thevenard <lucthevenard@gmail.com>
MIT License

Copyright (c) 2017 Dom Christie

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/
(() => {

  // node_modules/.pnpm/turndown@7.2.4/node_modules/turndown/lib/turndown.browser.es.js
  function extend(destination) {
    for (var i = 1;i < arguments.length; i++) {
      var source = arguments[i];
      for (var key in source) {
        if (Object.prototype.hasOwnProperty.call(source, key))
          destination[key] = source[key];
      }
    }
    return destination;
  }
  function repeat(character, count) {
    return Array(count + 1).join(character);
  }
  function trimLeadingNewlines(string) {
    return string.replace(/^\n*/, "");
  }
  function trimTrailingNewlines(string) {
    var indexEnd = string.length;
    while (indexEnd > 0 && string[indexEnd - 1] === `
`)
      indexEnd--;
    return string.substring(0, indexEnd);
  }
  function trimNewlines(string) {
    return trimTrailingNewlines(trimLeadingNewlines(string));
  }
  var blockElements = ["ADDRESS", "ARTICLE", "ASIDE", "AUDIO", "BLOCKQUOTE", "BODY", "CANVAS", "CENTER", "DD", "DIR", "DIV", "DL", "DT", "FIELDSET", "FIGCAPTION", "FIGURE", "FOOTER", "FORM", "FRAMESET", "H1", "H2", "H3", "H4", "H5", "H6", "HEADER", "HGROUP", "HR", "HTML", "ISINDEX", "LI", "MAIN", "MENU", "NAV", "NOFRAMES", "NOSCRIPT", "OL", "OUTPUT", "P", "PRE", "SECTION", "TABLE", "TBODY", "TD", "TFOOT", "TH", "THEAD", "TR", "UL"];
  function isBlock(node) {
    return is(node, blockElements);
  }
  var voidElements = ["AREA", "BASE", "BR", "COL", "COMMAND", "EMBED", "HR", "IMG", "INPUT", "KEYGEN", "LINK", "META", "PARAM", "SOURCE", "TRACK", "WBR"];
  function isVoid(node) {
    return is(node, voidElements);
  }
  function hasVoid(node) {
    return has(node, voidElements);
  }
  var meaningfulWhenBlankElements = ["A", "TABLE", "THEAD", "TBODY", "TFOOT", "TH", "TD", "IFRAME", "SCRIPT", "AUDIO", "VIDEO"];
  function isMeaningfulWhenBlank(node) {
    return is(node, meaningfulWhenBlankElements);
  }
  function hasMeaningfulWhenBlank(node) {
    return has(node, meaningfulWhenBlankElements);
  }
  function is(node, tagNames) {
    return tagNames.indexOf(node.nodeName) >= 0;
  }
  function has(node, tagNames) {
    return node.getElementsByTagName && tagNames.some(function(tagName) {
      return node.getElementsByTagName(tagName).length;
    });
  }
  var markdownEscapes = [[/\\/g, "\\\\"], [/\*/g, "\\*"], [/^-/g, "\\-"], [/^\+ /g, "\\+ "], [/^(=+)/g, "\\$1"], [/^(#{1,6}) /g, "\\$1 "], [/`/g, "\\`"], [/^~~~/g, "\\~~~"], [/\[/g, "\\["], [/\]/g, "\\]"], [/^>/g, "\\>"], [/_/g, "\\_"], [/^(\d+)\. /g, "$1\\. "]];
  function escapeMarkdown(string) {
    return markdownEscapes.reduce(function(accumulator, escape) {
      return accumulator.replace(escape[0], escape[1]);
    }, string);
  }
  var rules = {};
  rules.paragraph = {
    filter: "p",
    replacement: function(content) {
      return `

` + content + `

`;
    }
  };
  rules.lineBreak = {
    filter: "br",
    replacement: function(content, node, options) {
      return options.br + `
`;
    }
  };
  rules.heading = {
    filter: ["h1", "h2", "h3", "h4", "h5", "h6"],
    replacement: function(content, node, options) {
      var hLevel = Number(node.nodeName.charAt(1));
      if (options.headingStyle === "setext" && hLevel < 3) {
        var underline = repeat(hLevel === 1 ? "=" : "-", content.length);
        return `

` + content + `
` + underline + `

`;
      } else {
        return `

` + repeat("#", hLevel) + " " + content + `

`;
      }
    }
  };
  rules.blockquote = {
    filter: "blockquote",
    replacement: function(content) {
      content = trimNewlines(content).replace(/^/gm, "> ");
      return `

` + content + `

`;
    }
  };
  rules.list = {
    filter: ["ul", "ol"],
    replacement: function(content, node) {
      var parent = node.parentNode;
      if (parent.nodeName === "LI" && parent.lastElementChild === node) {
        return `
` + content;
      } else {
        return `

` + content + `

`;
      }
    }
  };
  rules.listItem = {
    filter: "li",
    replacement: function(content, node, options) {
      var prefix = options.bulletListMarker + "   ";
      var parent = node.parentNode;
      if (parent.nodeName === "OL") {
        var start = parent.getAttribute("start");
        var index = Array.prototype.indexOf.call(parent.children, node);
        prefix = (start ? Number(start) + index : index + 1) + ".  ";
      }
      var isParagraph = /\n$/.test(content);
      content = trimNewlines(content) + (isParagraph ? `
` : "");
      content = content.replace(/\n/gm, `
` + " ".repeat(prefix.length));
      return prefix + content + (node.nextSibling ? `
` : "");
    }
  };
  rules.indentedCodeBlock = {
    filter: function(node, options) {
      return options.codeBlockStyle === "indented" && node.nodeName === "PRE" && node.firstChild && node.firstChild.nodeName === "CODE";
    },
    replacement: function(content, node, options) {
      return `

    ` + node.firstChild.textContent.replace(/\n/g, `
    `) + `

`;
    }
  };
  rules.fencedCodeBlock = {
    filter: function(node, options) {
      return options.codeBlockStyle === "fenced" && node.nodeName === "PRE" && node.firstChild && node.firstChild.nodeName === "CODE";
    },
    replacement: function(content, node, options) {
      var className = node.firstChild.getAttribute("class") || "";
      var language = (className.match(/language-(\S+)/) || [null, ""])[1];
      var code = node.firstChild.textContent;
      var fenceChar = options.fence.charAt(0);
      var fenceSize = 3;
      var fenceInCodeRegex = new RegExp("^" + fenceChar + "{3,}", "gm");
      var match;
      while (match = fenceInCodeRegex.exec(code)) {
        if (match[0].length >= fenceSize) {
          fenceSize = match[0].length + 1;
        }
      }
      var fence = repeat(fenceChar, fenceSize);
      return `

` + fence + language + `
` + code.replace(/\n$/, "") + `
` + fence + `

`;
    }
  };
  rules.horizontalRule = {
    filter: "hr",
    replacement: function(content, node, options) {
      return `

` + options.hr + `

`;
    }
  };
  rules.inlineLink = {
    filter: function(node, options) {
      return options.linkStyle === "inlined" && node.nodeName === "A" && node.getAttribute("href");
    },
    replacement: function(content, node) {
      var href = escapeLinkDestination(node.getAttribute("href"));
      var title = escapeLinkTitle(cleanAttribute(node.getAttribute("title")));
      var titlePart = title ? ' "' + title + '"' : "";
      return "[" + content + "](" + href + titlePart + ")";
    }
  };
  rules.referenceLink = {
    filter: function(node, options) {
      return options.linkStyle === "referenced" && node.nodeName === "A" && node.getAttribute("href");
    },
    replacement: function(content, node, options) {
      var href = escapeLinkDestination(node.getAttribute("href"));
      var title = cleanAttribute(node.getAttribute("title"));
      if (title)
        title = ' "' + escapeLinkTitle(title) + '"';
      var replacement;
      var reference;
      switch (options.linkReferenceStyle) {
        case "collapsed":
          replacement = "[" + content + "][]";
          reference = "[" + content + "]: " + href + title;
          break;
        case "shortcut":
          replacement = "[" + content + "]";
          reference = "[" + content + "]: " + href + title;
          break;
        default:
          var id = this.references.length + 1;
          replacement = "[" + content + "][" + id + "]";
          reference = "[" + id + "]: " + href + title;
      }
      this.references.push(reference);
      return replacement;
    },
    references: [],
    append: function(options) {
      var references = "";
      if (this.references.length) {
        references = `

` + this.references.join(`
`) + `

`;
        this.references = [];
      }
      return references;
    }
  };
  rules.emphasis = {
    filter: ["em", "i"],
    replacement: function(content, node, options) {
      if (!content.trim())
        return "";
      return options.emDelimiter + content + options.emDelimiter;
    }
  };
  rules.strong = {
    filter: ["strong", "b"],
    replacement: function(content, node, options) {
      if (!content.trim())
        return "";
      return options.strongDelimiter + content + options.strongDelimiter;
    }
  };
  rules.code = {
    filter: function(node) {
      var hasSiblings = node.previousSibling || node.nextSibling;
      var isCodeBlock = node.parentNode.nodeName === "PRE" && !hasSiblings;
      return node.nodeName === "CODE" && !isCodeBlock;
    },
    replacement: function(content) {
      if (!content)
        return "";
      content = content.replace(/\r?\n|\r/g, " ");
      var extraSpace = /^`|^ .*?[^ ].* $|`$/.test(content) ? " " : "";
      var delimiter = "`";
      var matches = content.match(/`+/gm) || [];
      while (matches.indexOf(delimiter) !== -1)
        delimiter = delimiter + "`";
      return delimiter + extraSpace + content + extraSpace + delimiter;
    }
  };
  rules.image = {
    filter: "img",
    replacement: function(content, node) {
      var alt = escapeMarkdown(cleanAttribute(node.getAttribute("alt")));
      var src = escapeLinkDestination(node.getAttribute("src") || "");
      var title = cleanAttribute(node.getAttribute("title"));
      var titlePart = title ? ' "' + escapeLinkTitle(title) + '"' : "";
      return src ? "![" + alt + "]" + "(" + src + titlePart + ")" : "";
    }
  };
  function cleanAttribute(attribute) {
    return attribute ? attribute.replace(/(\n+\s*)+/g, `
`) : "";
  }
  function escapeLinkDestination(destination) {
    var escaped = destination.replace(/([<>()])/g, "\\$1");
    return escaped.indexOf(" ") >= 0 ? "<" + escaped + ">" : escaped;
  }
  function escapeLinkTitle(title) {
    return title.replace(/"/g, "\\\"");
  }
  function Rules(options) {
    this.options = options;
    this._keep = [];
    this._remove = [];
    this.blankRule = {
      replacement: options.blankReplacement
    };
    this.keepReplacement = options.keepReplacement;
    this.defaultRule = {
      replacement: options.defaultReplacement
    };
    this.array = [];
    for (var key in options.rules)
      this.array.push(options.rules[key]);
  }
  Rules.prototype = {
    add: function(key, rule) {
      this.array.unshift(rule);
    },
    keep: function(filter) {
      this._keep.unshift({
        filter,
        replacement: this.keepReplacement
      });
    },
    remove: function(filter) {
      this._remove.unshift({
        filter,
        replacement: function() {
          return "";
        }
      });
    },
    forNode: function(node) {
      if (node.isBlank)
        return this.blankRule;
      var rule;
      if (rule = findRule(this.array, node, this.options))
        return rule;
      if (rule = findRule(this._keep, node, this.options))
        return rule;
      if (rule = findRule(this._remove, node, this.options))
        return rule;
      return this.defaultRule;
    },
    forEach: function(fn) {
      for (var i = 0;i < this.array.length; i++)
        fn(this.array[i], i);
    }
  };
  function findRule(rules, node, options) {
    for (var i = 0;i < rules.length; i++) {
      var rule = rules[i];
      if (filterValue(rule, node, options))
        return rule;
    }
    return;
  }
  function filterValue(rule, node, options) {
    var filter = rule.filter;
    if (typeof filter === "string") {
      if (filter === node.nodeName.toLowerCase())
        return true;
    } else if (Array.isArray(filter)) {
      if (filter.indexOf(node.nodeName.toLowerCase()) > -1)
        return true;
    } else if (typeof filter === "function") {
      if (filter.call(rule, node, options))
        return true;
    } else {
      throw new TypeError("`filter` needs to be a string, array, or function");
    }
  }
  function collapseWhitespace(options) {
    var element = options.element;
    var isBlock = options.isBlock;
    var isVoid = options.isVoid;
    var isPre = options.isPre || function(node) {
      return node.nodeName === "PRE";
    };
    if (!element.firstChild || isPre(element))
      return;
    var prevText = null;
    var keepLeadingWs = false;
    var prev = null;
    var node = next(prev, element, isPre);
    while (node !== element) {
      if (node.nodeType === 3 || node.nodeType === 4) {
        var text = node.data.replace(/[ \r\n\t]+/g, " ");
        if ((!prevText || / $/.test(prevText.data)) && !keepLeadingWs && text[0] === " ") {
          text = text.substr(1);
        }
        if (!text) {
          node = remove(node);
          continue;
        }
        node.data = text;
        prevText = node;
      } else if (node.nodeType === 1) {
        if (isBlock(node) || node.nodeName === "BR") {
          if (prevText) {
            prevText.data = prevText.data.replace(/ $/, "");
          }
          prevText = null;
          keepLeadingWs = false;
        } else if (isVoid(node) || isPre(node)) {
          prevText = null;
          keepLeadingWs = true;
        } else if (prevText) {
          keepLeadingWs = false;
        }
      } else {
        node = remove(node);
        continue;
      }
      var nextNode = next(prev, node, isPre);
      prev = node;
      node = nextNode;
    }
    if (prevText) {
      prevText.data = prevText.data.replace(/ $/, "");
      if (!prevText.data) {
        remove(prevText);
      }
    }
  }
  function remove(node) {
    var next = node.nextSibling || node.parentNode;
    node.parentNode.removeChild(node);
    return next;
  }
  function next(prev, current, isPre) {
    if (prev && prev.parentNode === current || isPre(current)) {
      return current.nextSibling || current.parentNode;
    }
    return current.firstChild || current.nextSibling || current.parentNode;
  }
  var root = typeof window !== "undefined" ? window : {};
  function canParseHTMLNatively() {
    var Parser = root.DOMParser;
    var canParse = false;
    try {
      if (new Parser().parseFromString("", "text/html")) {
        canParse = true;
      }
    } catch (e) {}
    return canParse;
  }
  function createHTMLParser() {
    var Parser = function() {};
    {
      if (shouldUseActiveX()) {
        Parser.prototype.parseFromString = function(string) {
          var doc = new window.ActiveXObject("htmlfile");
          doc.designMode = "on";
          doc.open();
          doc.write(string);
          doc.close();
          return doc;
        };
      } else {
        Parser.prototype.parseFromString = function(string) {
          var doc = document.implementation.createHTMLDocument("");
          doc.open();
          doc.write(string);
          doc.close();
          return doc;
        };
      }
    }
    return Parser;
  }
  function shouldUseActiveX() {
    var useActiveX = false;
    try {
      document.implementation.createHTMLDocument("").open();
    } catch (e) {
      if (root.ActiveXObject)
        useActiveX = true;
    }
    return useActiveX;
  }
  var HTMLParser = canParseHTMLNatively() ? root.DOMParser : createHTMLParser();
  function RootNode(input, options) {
    var root;
    if (typeof input === "string") {
      var doc = htmlParser().parseFromString('<x-turndown id="turndown-root">' + input + "</x-turndown>", "text/html");
      root = doc.getElementById("turndown-root");
    } else {
      root = input.cloneNode(true);
    }
    collapseWhitespace({
      element: root,
      isBlock,
      isVoid,
      isPre: options.preformattedCode ? isPreOrCode : null
    });
    return root;
  }
  var _htmlParser;
  function htmlParser() {
    _htmlParser = _htmlParser || new HTMLParser;
    return _htmlParser;
  }
  function isPreOrCode(node) {
    return node.nodeName === "PRE" || node.nodeName === "CODE";
  }
  function Node2(node, options) {
    node.isBlock = isBlock(node);
    node.isCode = node.nodeName === "CODE" || node.parentNode.isCode;
    node.isBlank = isBlank(node);
    node.flankingWhitespace = flankingWhitespace(node, options);
    return node;
  }
  function isBlank(node) {
    return !isVoid(node) && !isMeaningfulWhenBlank(node) && /^\s*$/i.test(node.textContent) && !hasVoid(node) && !hasMeaningfulWhenBlank(node);
  }
  function flankingWhitespace(node, options) {
    if (node.isBlock || options.preformattedCode && node.isCode) {
      return {
        leading: "",
        trailing: ""
      };
    }
    var edges = edgeWhitespace(node.textContent);
    if (edges.leadingAscii && isFlankedByWhitespace("left", node, options)) {
      edges.leading = edges.leadingNonAscii;
    }
    if (edges.trailingAscii && isFlankedByWhitespace("right", node, options)) {
      edges.trailing = edges.trailingNonAscii;
    }
    return {
      leading: edges.leading,
      trailing: edges.trailing
    };
  }
  function edgeWhitespace(string) {
    var m = string.match(/^(([ \t\r\n]*)(\s*))(?:(?=\S)[\s\S]*\S)?((\s*?)([ \t\r\n]*))$/);
    return {
      leading: m[1],
      leadingAscii: m[2],
      leadingNonAscii: m[3],
      trailing: m[4],
      trailingNonAscii: m[5],
      trailingAscii: m[6]
    };
  }
  function isFlankedByWhitespace(side, node, options) {
    var sibling;
    var regExp;
    var isFlanked;
    if (side === "left") {
      sibling = node.previousSibling;
      regExp = / $/;
    } else {
      sibling = node.nextSibling;
      regExp = /^ /;
    }
    if (sibling) {
      if (sibling.nodeType === 3) {
        isFlanked = regExp.test(sibling.nodeValue);
      } else if (options.preformattedCode && sibling.nodeName === "CODE") {
        isFlanked = false;
      } else if (sibling.nodeType === 1 && !isBlock(sibling)) {
        isFlanked = regExp.test(sibling.textContent);
      }
    }
    return isFlanked;
  }
  var reduce = Array.prototype.reduce;
  function TurndownService(options) {
    if (!(this instanceof TurndownService))
      return new TurndownService(options);
    var defaults = {
      rules,
      headingStyle: "setext",
      hr: "* * *",
      bulletListMarker: "*",
      codeBlockStyle: "indented",
      fence: "```",
      emDelimiter: "_",
      strongDelimiter: "**",
      linkStyle: "inlined",
      linkReferenceStyle: "full",
      br: "  ",
      preformattedCode: false,
      blankReplacement: function(content, node) {
        return node.isBlock ? `

` : "";
      },
      keepReplacement: function(content, node) {
        return node.isBlock ? `

` + node.outerHTML + `

` : node.outerHTML;
      },
      defaultReplacement: function(content, node) {
        return node.isBlock ? `

` + content + `

` : content;
      }
    };
    this.options = extend({}, defaults, options);
    this.rules = new Rules(this.options);
  }
  TurndownService.prototype = {
    turndown: function(input) {
      if (!canConvert(input)) {
        throw new TypeError(input + " is not a string, or an element/document/fragment node.");
      }
      if (input === "")
        return "";
      var output = process.call(this, new RootNode(input, this.options));
      return postProcess.call(this, output);
    },
    use: function(plugin) {
      if (Array.isArray(plugin)) {
        for (var i = 0;i < plugin.length; i++)
          this.use(plugin[i]);
      } else if (typeof plugin === "function") {
        plugin(this);
      } else {
        throw new TypeError("plugin must be a Function or an Array of Functions");
      }
      return this;
    },
    addRule: function(key, rule) {
      this.rules.add(key, rule);
      return this;
    },
    keep: function(filter) {
      this.rules.keep(filter);
      return this;
    },
    remove: function(filter) {
      this.rules.remove(filter);
      return this;
    },
    escape: function(string) {
      return escapeMarkdown(string);
    }
  };
  function process(parentNode) {
    var self = this;
    return reduce.call(parentNode.childNodes, function(output, node) {
      node = new Node2(node, self.options);
      var replacement = "";
      if (node.nodeType === 3) {
        replacement = node.isCode ? node.nodeValue : self.escape(node.nodeValue);
      } else if (node.nodeType === 1) {
        replacement = replacementForNode.call(self, node);
      }
      return join(output, replacement);
    }, "");
  }
  function postProcess(output) {
    var self = this;
    this.rules.forEach(function(rule) {
      if (typeof rule.append === "function") {
        output = join(output, rule.append(self.options));
      }
    });
    return output.replace(/^[\t\r\n]+/, "").replace(/[\t\r\n\s]+$/, "");
  }
  function replacementForNode(node) {
    var rule = this.rules.forNode(node);
    var content = process.call(this, node);
    var whitespace = node.flankingWhitespace;
    if (whitespace.leading || whitespace.trailing)
      content = content.trim();
    return whitespace.leading + rule.replacement(content, node, this.options) + whitespace.trailing;
  }
  function join(output, replacement) {
    var s1 = trimTrailingNewlines(output);
    var s2 = trimLeadingNewlines(replacement);
    var nls = Math.max(output.length - s1.length, replacement.length - s2.length);
    var separator = `

`.substring(0, nls);
    return s1 + separator + s2;
  }
  function canConvert(input) {
    return input != null && (typeof input === "string" || input.nodeType && (input.nodeType === 1 || input.nodeType === 9 || input.nodeType === 11));
  }

  // node_modules/.pnpm/turndown-plugin-gfm@1.0.2/node_modules/turndown-plugin-gfm/lib/turndown-plugin-gfm.es.js
  function strikethrough(turndownService) {
    turndownService.addRule("strikethrough", {
      filter: ["del", "s", "strike"],
      replacement: function(content) {
        return "~" + content + "~";
      }
    });
  }
  var indexOf = Array.prototype.indexOf;
  var every = Array.prototype.every;
  var rules2 = {};
  rules2.tableCell = {
    filter: ["th", "td"],
    replacement: function(content, node) {
      return cell(content, node);
    }
  };
  rules2.tableRow = {
    filter: "tr",
    replacement: function(content, node) {
      var borderCells = "";
      var alignMap = { left: ":--", right: "--:", center: ":-:" };
      if (isHeadingRow(node)) {
        for (var i = 0;i < node.childNodes.length; i++) {
          var border = "---";
          var align = (node.childNodes[i].getAttribute("align") || "").toLowerCase();
          if (align)
            border = alignMap[align] || border;
          borderCells += cell(border, node.childNodes[i]);
        }
      }
      return `
` + content + (borderCells ? `
` + borderCells : "");
    }
  };
  rules2.table = {
    filter: function(node) {
      return node.nodeName === "TABLE" && isHeadingRow(node.rows[0]);
    },
    replacement: function(content) {
      content = content.replace(`

`, `
`);
      return `

` + content + `

`;
    }
  };
  rules2.tableSection = {
    filter: ["thead", "tbody", "tfoot"],
    replacement: function(content) {
      return content;
    }
  };
  function isHeadingRow(tr) {
    var parentNode = tr.parentNode;
    return parentNode.nodeName === "THEAD" || parentNode.firstChild === tr && (parentNode.nodeName === "TABLE" || isFirstTbody(parentNode)) && every.call(tr.childNodes, function(n) {
      return n.nodeName === "TH";
    });
  }
  function isFirstTbody(element) {
    var previousSibling = element.previousSibling;
    return element.nodeName === "TBODY" && (!previousSibling || previousSibling.nodeName === "THEAD" && /^\s*$/i.test(previousSibling.textContent));
  }
  function cell(content, node) {
    var index = indexOf.call(node.parentNode.childNodes, node);
    var prefix = " ";
    if (index === 0)
      prefix = "| ";
    return prefix + content + " |";
  }
  function tables(turndownService) {
    turndownService.keep(function(node) {
      return node.nodeName === "TABLE" && !isHeadingRow(node.rows[0]);
    });
    for (var key in rules2)
      turndownService.addRule(key, rules2[key]);
  }
  function taskListItems(turndownService) {
    turndownService.addRule("taskListItems", {
      filter: function(node) {
        return node.type === "checkbox" && node.parentNode.nodeName === "LI";
      },
      replacement: function(content, node) {
        return (node.checked ? "[x]" : "[ ]") + " ";
      }
    });
  }

  // src/web-read/entry.ts
  function hidesSubtree(element, style) {
    const parent = element.parentElement;
    return element.matches("script, style, template, noscript, [hidden]") || style.display === "none" || style.contentVisibility === "hidden" || parent instanceof HTMLDetailsElement && !parent.open && element !== parent.querySelector(":scope > summary");
  }
  function renderedChildren(element) {
    if (element instanceof HTMLSlotElement) {
      const assigned = element.assignedNodes();
      return assigned.length ? assigned : [...element.childNodes];
    }
    return [...(element.shadowRoot ?? element).childNodes];
  }
  function renderedParent(element) {
    if (element.assignedSlot)
      return element.assignedSlot;
    const parent = element.parentNode;
    return parent instanceof ShadowRoot ? parent.host : element.parentElement;
  }
  var BLOCK_DISPLAY = /^(block|flow-root|flex|grid|list-item|table|table-row|table-caption)$/;
  var FLEX_GRID_DISPLAY = /^(flex|inline-flex|grid|inline-grid)$/;
  var NATIVE_BLOCK_TAGS = new Set([
    "ADDRESS",
    "ARTICLE",
    "ASIDE",
    "BLOCKQUOTE",
    "BR",
    "CAPTION",
    "DD",
    "DETAILS",
    "DIALOG",
    "DIV",
    "DL",
    "DT",
    "FIELDSET",
    "FIGCAPTION",
    "FIGURE",
    "FOOTER",
    "FORM",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "HEADER",
    "HGROUP",
    "HR",
    "LI",
    "MAIN",
    "NAV",
    "OL",
    "P",
    "PRE",
    "SECTION",
    "SUMMARY",
    "TABLE",
    "TBODY",
    "TD",
    "TFOOT",
    "TH",
    "THEAD",
    "TR",
    "UL"
  ]);
  function copyVisibleContent(root) {
    const inertDocument = document.implementation.createHTMLDocument("");
    const container = inertDocument.createElement("div");
    function copyElement(original, inCode) {
      const style = getComputedStyle(original);
      if (hidesSubtree(original, style))
        return null;
      const copy = inertDocument.importNode(original, false);
      const visible = style.visibility !== "hidden" && style.visibility !== "collapse";
      const closed = original instanceof HTMLDetailsElement && !original.open;
      const insideCode = inCode || original.tagName === "PRE" || original.tagName === "CODE";
      const isFlexOrGrid = !insideCode && FLEX_GRID_DISPLAY.test(style.display);
      let text = "";
      let prevSeparated = false;
      for (const child of renderedChildren(original)) {
        if (child instanceof Element) {
          const result = copyElement(child, insideCode);
          if (result) {
            const hasText = Boolean(result.copy.textContent?.trim());
            if (!insideCode && copy.lastChild && (prevSeparated || isFlexOrGrid && hasText) && !/\s$/.test(copy.lastChild.textContent ?? "")) {
              copy.append(" ");
            }
            copy.append(result.copy);
            text += result.text;
            prevSeparated = result.separate || isFlexOrGrid && hasText;
          }
        } else if (child.nodeType === Node.TEXT_NODE && visible && !closed) {
          const value = child.textContent ?? "";
          if (!insideCode && prevSeparated && value && !/^\s/.test(value)) {
            copy.append(" ");
          }
          copy.append(inertDocument.importNode(child, false));
          text += /^(pre|break-spaces)/.test(style.whiteSpace) ? value : value.replace(/[\t\n\r\f ]+/g, " ");
          if (value)
            prevSeparated = false;
        }
      }
      if ((!visible || closed) && copy.childElementCount === 0)
        return null;
      if (original instanceof HTMLAnchorElement && original.hasAttribute("href")) {
        copy.setAttribute("href", original.href);
      }
      if (original instanceof HTMLImageElement && original.hasAttribute("src")) {
        copy.setAttribute("src", original.currentSrc || original.src);
      }
      if (visible && original instanceof HTMLBRElement)
        text = `
`;
      if (text && BLOCK_DISPLAY.test(style.display)) {
        text = (text.startsWith(`
`) ? "" : `
`) + text + (text.endsWith(`
`) ? "" : `
`);
      } else if (text && style.display === "table-cell") {
        text += "\t";
      }
      const separate = !insideCode && BLOCK_DISPLAY.test(style.display) && !NATIVE_BLOCK_TAGS.has(original.tagName);
      return { copy, text, separate };
    }
    const result = copyElement(root, false);
    if (result)
      container.append(result.copy);
    return { container, text: result?.text ?? "" };
  }
  function text() {
    if (!document.body)
      return document.documentElement?.textContent ?? "";
    if (![document.body, ...document.body.querySelectorAll("*")].some((element) => element.shadowRoot)) {
      return document.body.innerText;
    }
    return copyVisibleContent(document.body).text.replace(/^\n+|\n+$/g, "");
  }
  function read(within = null) {
    if (!document.body) {
      throw new Error("Markdown reading requires a document with an HTML body");
    }
    let root = document.body;
    if (within !== null) {
      if (within.startsWith("@")) {
        const resolveRef = window.__komaWebDOM?.komaResolveRef;
        const target = typeof resolveRef === "function" ? resolveRef(within) : null;
        if (!(target instanceof Element))
          throw new Error("Scope not found: " + within);
        root = target;
      } else {
        const matches = document.querySelectorAll(within);
        if (matches.length === 0)
          throw new Error("Scope not found: " + within);
        if (matches.length > 1)
          throw new Error("Scope matched multiple elements: " + within);
        root = matches[0];
      }
    }
    for (let ancestor = root;ancestor; ancestor = renderedParent(ancestor)) {
      if (hidesSubtree(ancestor, getComputedStyle(ancestor)) || ancestor.parentElement?.shadowRoot && !ancestor.assignedSlot) {
        return { title: document.title, url: location.href, markdown: "" };
      }
    }
    const { container } = copyVisibleContent(root);
    const inertDocument = container.ownerDocument;
    for (const table of container.querySelectorAll("table")) {
      if (table.rows.length === 0)
        table.replaceWith(...table.childNodes);
    }
    for (const pre of container.querySelectorAll("pre")) {
      const existing = pre.querySelector(":scope > code");
      const language = pre.parentElement?.className.match(/(?:^|\s)highlight-(?:text|source)-(\S+)/)?.[1];
      const code = inertDocument.createElement("code");
      code.className = existing?.className || (language ? "language-" + language : "");
      code.textContent = existing?.textContent ?? pre.textContent;
      pre.replaceChildren(code);
    }
    const converter = new TurndownService({ headingStyle: "atx", codeBlockStyle: "fenced" });
    converter.use([strikethrough, tables, taskListItems]);
    converter.addRule("escapedTableCell", {
      filter: ["th", "td"],
      replacement: (content, node) => {
        const prefix = node.previousElementSibling ? " " : "| ";
        const cell = content.trim().replace(/\|/g, "\\|").replace(/[ \t]*(?:\r\n?|\n)/g, "<br>");
        return prefix + cell + " |";
      }
    });
    return { title: document.title, url: location.href, markdown: converter.turndown(container) };
  }

  // src/web-read/browser.ts
  Object.assign(globalThis, { komaWebRead: { read, text } });
})();
