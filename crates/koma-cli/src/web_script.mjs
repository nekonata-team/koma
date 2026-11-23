import { Console } from "node:console";
import { readFileSync } from "node:fs";
import { inspect } from "node:util";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const [file, port, targetId] = process.argv.slice(1);
const snapshotExpression = process.env.KOMA_SCRIPT_SNAPSHOT;
delete process.env.KOMA_SCRIPT_SNAPSHOT;
globalThis.console = new Console(process.stderr, process.stderr);

async function matchesTarget(page) {
  let session;
  try {
    session = await page.context().newCDPSession(page);
    const { targetInfo } = await session.send("Target.getTargetInfo");
    return targetInfo.targetId === targetId;
  } catch (error) {
    if (page.isClosed()) return false;
    throw error;
  } finally {
    if (session) await session.detach().catch(() => {});
  }
}

async function run() {
  const scriptUrl = pathToFileURL(resolve(file));
  const require = createRequire(scriptUrl);
  let playwrightPath;
  try {
    playwrightPath = require.resolve("playwright-core");
  } catch (error) {
    if (error.code !== "MODULE_NOT_FOUND") throw error;
    throw new Error("Install playwright-core (1.60 or newer) in the script's project");
  }
  const [major, minor] = require("playwright-core/package.json").version.split(".").map(Number);
  if (major === 1 && minor < 60) {
    throw new Error("koma web script requires playwright-core 1.60 or newer");
  }
  const { chromium } = require(playwrightPath);
  const source = file === "-"
    ? `data:text/javascript;base64,${readFileSync(0).toString("base64")}`
    : scriptUrl.href;
  const { default: script } = await import(source);
  if (typeof script !== "function") {
    throw new Error("Expected a default export of a function in the Web Koma script");
  }
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, {
    isLocal: true,
    noDefaults: true,
  });
  try {
    const pages = browser.contexts().flatMap((context) => context.pages());
    const matches = await Promise.all(pages.map(matchesTarget));
    const page = pages[matches.indexOf(true)];
    if (!page || page.isClosed()) throw new Error("The selected Web Koma is no longer open");
    const session = await page.context().newCDPSession(page);
    // CEF pauses animation frames for hidden Komas. Playwright waits for
    // stable geometry, so keep only this target active for this CDP session.
    await session.send("Emulation.setFocusEmulationEnabled", { enabled: true });
    const snapshot = async () => {
      const result = await page.evaluate(snapshotExpression);
      if (!result.ok) throw new Error(result.error ?? "Web Koma snapshot failed");
      return result.snapshot;
    };
    const closed = new AbortController();
    const failOnClose = new Promise((_, reject) => {
      const onClose = () => reject(new Error("The selected Web Koma closed"));
      page.once("close", onClose);
      browser.once("disconnected", onClose);
      closed.signal.addEventListener("abort", () => {
        page.off("close", onClose);
        browser.off("disconnected", onClose);
      }, { once: true });
    });
    try {
      return await Promise.race([Promise.resolve().then(() => script({ page, snapshot })), failOnClose]);
    } finally {
      closed.abort();
      await session.detach().catch(() => {});
    }
  } finally {
    await browser.close();
  }
}

try {
  const result = await run();
  await write(process.stdout, `${JSON.stringify(result) ?? "null"}\n`);
  await write(process.stderr, "");
  process.exit(0);
} catch (error) {
  await write(process.stderr, `${inspect(error)}\n`);
  process.exit(1);
}

function write(stream, text) {
  return new Promise((resolve, reject) => {
    stream.write(text, (error) => error ? reject(error) : resolve());
  });
}
