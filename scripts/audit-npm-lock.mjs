#!/usr/bin/env node
import fs from "node:fs";

const allowedLicenses = new Set([
  "0BSD",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "CDLA-Permissive-2.0",
  "ISC",
  "MIT",
  "MPL-2.0",
  "NCSA",
  "Python-2.0",
  "Unicode-3.0",
  "Zlib",
]);

const lockPath = process.argv[2] ?? "pnpm-lock.yaml";
const lock = fs.readFileSync(lockPath, "utf8");
const packageKeys = [];
let inPackages = false;

for (const line of lock.split("\n")) {
  if (line === "packages:") {
    inPackages = true;
    continue;
  }
  if (inPackages && /^\S/.test(line)) break;
  const match = line.match(/^ {2}([^ ][^:]+):$/);
  if (inPackages && match) packageKeys.push(match[1].replace(/^'|'$/g, ""));
}

const packages = new Map();
for (const key of packageKeys) {
  const base = key.replace(/\(.*/, "");
  const at = base.lastIndexOf("@");
  if (at <= 0) continue;
  packages.set(`${base.slice(0, at)}@${base.slice(at + 1)}`, {
    name: base.slice(0, at),
    version: base.slice(at + 1),
  });
}

const failures = [];
const cache = new Map();

function npmPackageUrl(name) {
  return `https://registry.npmjs.org/${name.replace("/", "%2f")}`;
}

function normalizeLicense(value) {
  if (!value) return "";
  if (typeof value === "string") return value.trim();
  if (typeof value === "object" && typeof value.type === "string") return value.type.trim();
  return "";
}

function expressionAllowed(expression) {
  const normalized = expression
    .replace(/^SEE LICENSE IN .*/i, "")
    .replace(/\bUNLICENSED\b/i, "UNLICENSED")
    .trim();
  if (!normalized) return false;

  const tokens = normalized.match(/[()]|AND|OR|WITH|[A-Za-z0-9.+-]+/g) ?? [];
  let index = 0;

  function parsePrimary() {
    const token = tokens[index++];
    if (!token) return false;
    if (token === "(") {
      const value = parseOr();
      if (tokens[index] === ")") index++;
      return value;
    }
    if (tokens[index] === "WITH") {
      index++;
      const exception = tokens[index++];
      return allowedLicenses.has(`${token} WITH ${exception}`);
    }
    return allowedLicenses.has(token);
  }

  function parseAnd() {
    let value = parsePrimary();
    while (tokens[index] === "AND") {
      index++;
      value = parsePrimary() && value;
    }
    return value;
  }

  function parseOr() {
    let value = parseAnd();
    while (tokens[index] === "OR") {
      index++;
      value = parseAnd() || value;
    }
    return value;
  }

  return parseOr();
}

async function fetchPackument(name) {
  if (cache.has(name)) return cache.get(name);
  const response = await fetch(npmPackageUrl(name));
  if (!response.ok) throw new Error(`npm registry returned ${response.status}`);
  const data = await response.json();
  cache.set(name, data);
  return data;
}

await Promise.all(
  [...packages.values()].map(async ({ name, version }) => {
    try {
      const packument = await fetchPackument(name);
      const metadata = packument.versions?.[version];
      if (!metadata) {
        failures.push(`${name}@${version}: version is missing from npm registry`);
        return;
      }
      if (metadata.deprecated) {
        failures.push(`${name}@${version}: deprecated: ${metadata.deprecated}`);
      }
      const license = normalizeLicense(metadata.license);
      if (!expressionAllowed(license)) {
        failures.push(`${name}@${version}: license is not allowed or missing: ${license || "<missing>"}`);
      }
    } catch (error) {
      failures.push(`${name}@${version}: ${error.message}`);
    }
  }),
);

if (failures.length > 0) {
  console.error("npm dependency policy violations:");
  for (const failure of failures.sort()) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`npm dependency policy ok (${packages.size} package versions checked)`);
