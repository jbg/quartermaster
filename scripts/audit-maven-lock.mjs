#!/usr/bin/env node
import fs from "node:fs";

const allowedLicenseNames = [
  /^(the )?apache( software)? license,? version 2\.0$/i,
  /^apache license v2\.0$/i,
  /^apache 2(\.0)?$/i,
  /^apache-2\.0$/i,
  /^android software development kit license$/i,
  /^android software development kit license agreement$/i,
  /^bsd( |-)2-clause/i,
  /^bsd( |-)3-clause/i,
  /^bsd license$/i,
  /^eclipse distribution license/i,
  /^eclipse public license/i,
  /^ml kit terms of service$/i,
  /^mit( license)?$/i,
  /^public domain$/i,
  /^bouncy castle licen[cs]e$/i,
  /^cddl \+ gplv2 with classpath exception$/i,
  /^common development and distribution license/i,
  /^gpl2 w\/ cpe$/i,
  /^gpl2 with classpath exception$/i,
  /^the bouncy castle licen[cs]e$/i,
  /^the bsd license$/i,
  /^the mit license$/i,
  /^unicode/i,
];

const lockPath = process.argv[2] ?? "android/app/gradle.lockfile";
const lock = fs.readFileSync(lockPath, "utf8");
const coordinates = [
  ...new Set(
    lock
      .split("\n")
      .map((line) => line.split("=")[0])
      .filter((line) => line && !line.startsWith("#") && line.split(":").length === 3),
  ),
].sort();

const repositories = [
  "https://dl.google.com/dl/android/maven2",
  "https://repo.maven.apache.org/maven2",
];
const fetchConcurrency = 12;
const fetchAttempts = 4;
const fetchTimeoutMs = 20_000;
const failures = [];

function pomPath(group, artifact, version) {
  return `${group.replaceAll(".", "/")}/${artifact}/${version}/${artifact}-${version}.pom`;
}

async function fetchPom(group, artifact, version) {
  const path = pomPath(group, artifact, version);
  const errors = [];
  for (const repo of repositories) {
    const url = `${repo}/${path}`;
    try {
      const response = await fetchWithRetry(url);
      if (response.ok) return { url, text: await response.text() };
      errors.push(`${url} (${response.status})`);
    } catch (error) {
      errors.push(`${url} (${error.message})`);
    }
  }
  throw new Error(`POM not found in configured Maven repositories: ${errors.join(", ")}`);
}

function retryableStatus(status) {
  return status === 408 || status === 429 || status >= 500;
}

function retryDelayMs(attempt) {
  return 250 * 2 ** (attempt - 1);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function fetchWithRetry(url) {
  for (let attempt = 1; attempt <= fetchAttempts; attempt++) {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(fetchTimeoutMs) });
      if (!retryableStatus(response.status) || attempt === fetchAttempts) return response;
    } catch (error) {
      if (attempt === fetchAttempts) {
        throw new Error(`${error.message} after ${fetchAttempts} attempts`);
      }
    }
    await sleep(retryDelayMs(attempt));
  }
}

async function forEachWithConcurrency(items, concurrency, callback) {
  let index = 0;
  const workers = Array.from({ length: Math.min(concurrency, items.length) }, async () => {
    while (index < items.length) {
      const item = items[index++];
      await callback(item);
    }
  });
  await Promise.all(workers);
}

function licenseNames(pom) {
  const names = [];
  const licenseBlocks = pom.match(/<license>[\s\S]*?<\/license>/g) ?? [];
  for (const block of licenseBlocks) {
    const match = block.match(/<name>\s*([\s\S]*?)\s*<\/name>/);
    if (match) names.push(match[1].replace(/\s+/g, " ").trim());
  }
  return names;
}

function allowed(name) {
  return allowedLicenseNames.some((pattern) => pattern.test(name));
}

await forEachWithConcurrency(
  coordinates,
  fetchConcurrency,
  async (coordinate) => {
    const [group, artifact, version] = coordinate.split(":");
    try {
      const { text } = await fetchPom(group, artifact, version);
      const names = licenseNames(text);
      if (names.length > 0 && !names.some(allowed)) {
        failures.push(`${coordinate}: license is not allowed: ${names.join(" OR ")}`);
      }
    } catch (error) {
      failures.push(`${coordinate}: ${error.message}`);
    }
  },
);

if (failures.length > 0) {
  console.error("Maven dependency policy violations:");
  for (const failure of failures.sort()) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`Maven dependency policy ok (${coordinates.length} package versions checked)`);
