#!/usr/bin/env node
import fs from "node:fs";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const allowedLicenses = new Set([
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "MIT",
  "MPL-2.0",
  "Unicode-3.0",
]);

const lockPath =
  process.argv[2] ??
  "ios/Quartermaster.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved";
const resolved = JSON.parse(fs.readFileSync(lockPath, "utf8"));
const failures = [];
const token = process.env.GITHUB_TOKEN;
const execFileAsync = promisify(execFile);

function githubRepo(location) {
  const match = location.match(/^https:\/\/github\.com\/([^/]+)\/([^/.]+)(?:\.git)?$/i);
  if (!match) return null;
  return { owner: match[1], repo: match[2] };
}

async function github(path) {
  const response = await fetch(`https://api.github.com${path}`, {
    headers: {
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
  });
  if (!response.ok) {
    const error = new Error(`GitHub API returned ${response.status} for ${path}`);
    error.status = response.status;
    throw error;
  }
  return response.json();
}

async function verifyVersionTag(pin, label) {
  const version = pin.state.version;
  const tags = [`refs/tags/${version}`, `refs/tags/v${version}`];
  const args = ["ls-remote", "--exit-code", "--tags", pin.location, ...tags];
  const { stdout } = await execFileAsync("git", args, { maxBuffer: 1024 * 1024 });
  const hashes = stdout
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => line.split(/\s+/, 1)[0]);

  if (!hashes.includes(pin.state.revision)) {
    failures.push(`${label}: version tag exists but does not resolve to pinned revision`);
  }
}

await Promise.all(
  resolved.pins.map(async (pin) => {
    const repo = githubRepo(pin.location);
    const label = `${pin.identity}@${pin.state.version ?? pin.state.revision}`;
    if (!repo) {
      failures.push(`${label}: unsupported non-GitHub SwiftPM source ${pin.location}`);
      return;
    }
    try {
      const metadata = await github(`/repos/${repo.owner}/${repo.repo}`);
      if (metadata.archived || metadata.disabled) {
        failures.push(`${label}: repository is archived or disabled`);
      }
      const spdx = metadata.license?.spdx_id;
      if (!spdx || !allowedLicenses.has(spdx)) {
        failures.push(`${label}: license is not allowed or missing: ${spdx || "<missing>"}`);
      }
      if (pin.state.version) {
        await verifyVersionTag(pin, label);
      } else {
        await github(`/repos/${repo.owner}/${repo.repo}/commits/${pin.state.revision}`);
      }
    } catch (error) {
      if (!token && error.status === 403 && pin.state.version) {
        await verifyVersionTag(pin, label);
        console.warn(`${label}: skipped GitHub metadata check after anonymous API rate limit`);
        return;
      }
      failures.push(`${label}: ${error.message}`);
    }
  }),
);

if (failures.length > 0) {
  console.error("SwiftPM dependency policy violations:");
  for (const failure of failures.sort()) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`SwiftPM dependency policy ok (${resolved.pins.length} package versions checked)`);
