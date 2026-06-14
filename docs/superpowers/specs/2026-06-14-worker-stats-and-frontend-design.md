# Worker Stats Persistence + Frontend Refresh — Design

**Date:** 2026-06-14
**Status:** Approved

## Goals

1. **Stop worker stats being lost on reconnect.** Today the master keys each
   worker row by a fresh per-connection UUID and deletes the row on disconnect,
   so a brief drop resets `tasks_done`, `avg_det_time`, and `connected_at`.
2. **Bump the frontend dependencies to latest stable.**
3. **Redesign the per-worker stats cards.**

This work lands in `rs-back` (the canonical backend) and the Vue frontend. The
legacy Python `back/` is left as-is. It continues on the `rust-backend-rewrite`
branch, since the backend change depends on `rs-back` existing.

## Part 1 — Backend: worker stats persistence (`rs-back`)

### Identity model

- Key the worker row by **name** (`id = name`) instead of a per-connection UUID.
  Worker names come from the worker's `pong` reply (operator-assigned via argv in
  `rs-worker`) and are meant to be unique per machine.
- On connect, the `Worker` struct is created, then the first `ping` yields the
  name. After that, load any existing row for that name and seed:
  - `tasks_done` and `avg_det_time` — so accumulation continues across reconnects.
  - `connected_at` — preserve the **first-ever** connection time. If no row
    exists, set `connected_at = now`.
- `sync_to_db` upserts by name (`ON CONFLICT (id) DO UPDATE`), preserving
  `connected_at` and writing the accumulated `tasks_done`/`avg_det_time` and a
  fresh `last_ping`.

### Online detection + lifecycle

- A worker is **online** iff `last_ping >= now - ONLINE_SECS`, with
  `ONLINE_SECS = 60`. (60s, up from today's 30s, so a worker that goes quiet for
  up to ~20s during a long detection does not flap offline.)
- **Remove** the on-disconnect row deletion (`cleanup`), so stats survive a drop.
  A dropped worker simply stops updating `last_ping` and ages off the online list.
- **Replace** the 30s expirer with a **1-week purge**:
  `DELETE FROM workers WHERE last_ping < now - 604800`. Offline rows linger up to
  a week so a reconnect within that window restores accumulated stats; after a
  week of silence the row is purged.

### API

- `GET /api/workers` returns **online workers only**:
  `SELECT id, name, connected_at, avg_det_time, last_ping, tasks_done FROM workers
   WHERE last_ping >= $1 ORDER BY last_ping DESC` (with `$1 = now - 60`).
- Response shape is **unchanged** (`{ "data": [...] }`), so the frontend contract
  holds and either backend can serve it.

### No schema change

`id` is repurposed as the name; all existing columns are reused. `ensure_tables`
is unchanged. (An explicit `online` boolean was rejected: it still needs a
timeout fallback for ungraceful TCP drops, so `last_ping` recency is the robust
signal and avoids a migration.)

### Edge cases

- Two live connections sharing one name share/clobber a single row. Accepted —
  names are meant unique; documented in a code comment.
- `avg_det_time` accumulation uses `self.tasks_done` (seeded from the DB), so the
  running mean stays correct across reconnects.

### Tests

- Unit test the online-threshold SQL parameter logic and the
  seed-from-existing-row accumulation (a small helper that, given an optional
  prior row, produces the seeded `(tasks_done, avg_det_time, connected_at)`).
- Existing router/no-DB tests remain green.

## Part 2 — Frontend: dependency bump

- Bump every entry in `package.json` to its latest stable release:
  `vue`, `vue-router` (the current `^5.0.3` is invalid — pin the real latest 4.x),
  `lodash`, `vue-confetti-explosion`, `naive-ui`, `openseadragon`,
  `@vicons/fluent`, `@vitejs/plugin-vue`, `vite`.
- Reinstall with `pnpm install`, resolve any breaking changes (most likely in
  `naive-ui`, `openseadragon`, or `vite` majors), and verify both
  `pnpm build` and `pnpm dev` run clean.

## Part 3 — Frontend: worker card redesign (`Workers.vue`)

- Rework the per-worker cards rendered inside the existing "计算节点" card on
  `HomeView`. Since `/api/workers` now returns only online workers, every card
  shown is live.
- Each card shows: worker name with a live indicator, first-connection relative
  time (`connected_at`), last-seen relative time (`last_ping`), tasks done,
  average detection time, and a derived throughput (tasks per hour of uptime,
  `tasks_done / max(1, (last_ping - connected_at)) * 3600`).
- Keep the existing empty-state ("无计算节点，服务中止！") and the 5s polling.
- Guard against `avg_det_time` being `null`/absent (use `?? 0` before
  `toFixed`) — today's `worker.avg_det_time.toFixed(2)` would throw on a null.

## Out of scope

- No changes to the legacy Python `back/`.
- No new aggregate-summary section (user chose per-card redesign only).
- No change to the worker (`rs-worker`) or the wire protocol.
