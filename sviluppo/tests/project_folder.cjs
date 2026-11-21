'use strict';
// Resolve a project's folder without assuming the legacy `<uuid>` name.
// New and migrated projects live in readable `<title>--<short id>` folders
// whose ownership marker is `.project-id` (with project.json as fallback).
const fs = require('node:fs');
const path = require('node:path');

function projectDir(data, id) {
  const base = path.join(data, 'projects');
  const legacy = path.join(base, id);
  let entries = [];
  try {
    entries = fs.readdirSync(base, { withFileTypes: true });
  } catch {
    return legacy;
  }
  if (entries.some(e => e.isDirectory() && e.name === id)) return legacy;
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const dir = path.join(base, entry.name);
    try {
      const marker = path.join(dir, '.project-id');
      if (fs.existsSync(marker) && fs.readFileSync(marker, 'utf8').trim() === id) return dir;
    } catch {}
    try {
      const record = path.join(dir, 'project.json');
      if (fs.existsSync(record) && JSON.parse(fs.readFileSync(record, 'utf8')).id === id) return dir;
    } catch {}
  }
  return legacy;
}

function firstProjectId(data) {
  const base = path.join(data, 'projects');
  for (const entry of fs.readdirSync(base, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const dir = path.join(base, entry.name);
    try {
      const marker = path.join(dir, '.project-id');
      if (fs.existsSync(marker)) return fs.readFileSync(marker, 'utf8').trim();
    } catch {}
    try {
      const record = path.join(dir, 'project.json');
      if (fs.existsSync(record)) return JSON.parse(fs.readFileSync(record, 'utf8')).id;
    } catch {}
  }
  return null;
}

module.exports = { projectDir, firstProjectId };
