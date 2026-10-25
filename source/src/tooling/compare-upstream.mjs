#!/usr/bin/env node
import {
  existsSync,
  lstatSync,
  readFileSync,
  readdirSync,
  statSync,
} from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { sha256, sourceSnapshot, writeReceipt } from "./build-provenance.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const slash = (path) => path.split(sep).join("/");
const ignored = new Set([".git", "node_modules", "target", "dist", ".build"]);
function files(folder, prefix = "") {
  return readdirSync(join(folder, prefix), { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name))
    .flatMap((entry) => {
      if (ignored.has(entry.name)) return [];
      if (entry.isSymbolicLink())
        throw Error(`Reference contains a symlink: ${entry.name}`);
      const path = slash(join(prefix, entry.name));
      return entry.isDirectory() ? files(folder, path) : [path];
    });
}
export function compareUpstream(sourceRoot, reference) {
  if (
    !existsSync(reference) ||
    !statSync(reference).isDirectory() ||
    lstatSync(reference).isSymbolicLink()
  )
    throw Error("Reference must be an existing plain source directory");
  const snapshot = sourceSnapshot(sourceRoot);
  // The repository path can differ after a clone; resolve it from Git's own root.
  const sourceRows = snapshot.files.filter((row) =>
    row.path.startsWith("source/"),
  );
  const local = new Set(
    sourceRows.map((row) => row.path.slice("source/".length)),
  );
  const upstream = files(reference),
    known = new Set(upstream);
  const same = [],
    changed = [],
    newlineOnly = [],
    localOnly = [...local].filter((path) => !known.has(path));
  const normalize = (bytes) =>
    bytes.includes(0)
      ? bytes
      : Buffer.from(
          bytes
            .toString("utf8")
            .replace(/^\uFEFF/, "")
            .replace(/\r\n/g, "\n"),
        );
  const fingerprints = [];
  for (const path of upstream) {
    const b = readFileSync(join(reference, path));
    fingerprints.push({ path, sha256: sha256(b) });
    if (!local.has(path)) continue;
    if (!existsSync(join(sourceRoot, path))) {
      changed.push(path);
      continue;
    }
    const a = readFileSync(join(sourceRoot, path));
    if (a.equals(b)) same.push(path);
    else if (normalize(a).equals(normalize(b))) newlineOnly.push(path);
    else changed.push(path);
  }
  const after = sourceSnapshot(sourceRoot);
  if (
    snapshot.inputSha256 !== after.inputSha256 ||
    snapshot.head !== after.head
  )
    throw Error(
      "Local source changed during comparison; retry after it settles",
    );
  const upstreamOnly = upstream.filter((path) => !local.has(path));
  return {
    schemaVersion: 1,
    checkedAt: new Date().toISOString(),
    scope: "content comparison only; no proof of Git ancestry",
    source: {
      head: snapshot.head,
      dirty: snapshot.dirty,
      inputSha256: snapshot.inputSha256,
    },
    reference: {
      directory: resolve(reference),
      treeSha256: sha256(JSON.stringify(fingerprints)),
    },
    counts: {
      identical: same.length,
      newlineOnly: newlineOnly.length,
      changed: changed.length,
      localOnly: localOnly.length,
      upstreamOnly: upstreamOnly.length,
    },
    changed,
    localOnly,
    upstreamOnly,
    newlineOnly,
  };
}

if (
  process.argv[1] &&
  pathToFileURL(resolve(process.argv[1])).href === import.meta.url
) {
  try {
    const args = process.argv.slice(2);
    if (
      ![2, 4].includes(args.length) ||
      args[0] !== "--reference" ||
      (args.length === 4 && args[2] !== "--output")
    )
      throw Error(
        "Usage: compare-upstream.mjs --reference source-directory [--output new-report.json]",
      );
    const report = compareUpstream(root, resolve(args[1]));
    if (args[3]) {
      writeReceipt(resolve(args[3]), report);
      process.stdout.write(JSON.stringify(report.counts) + "\n");
    } else process.stdout.write(JSON.stringify(report, null, 2) + "\n");
  } catch (error) {
    process.stderr.write(`Upstream comparison failed: ${error.message}\n`);
    process.exitCode = 1;
  }
}
