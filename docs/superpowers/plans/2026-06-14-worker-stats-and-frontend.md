# Worker Stats Persistence + Frontend Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `rs-back` persist worker stats across reconnects (keyed by worker name, online-only display, 1-week purge), bump the Vue frontend deps to latest, and redesign the per-worker stats cards.

**Architecture:** In `rs-back/src/master.rs` the per-connection UUID identity is replaced by the worker's name as the row key; stats are seeded from any existing row on connect and accumulate. Online-ness is derived from `last_ping` recency (60s), the expirer becomes a 1-week purge, and `GET /api/workers` filters to online rows. The frontend bumps `package.json` and reworks `Workers.vue`.

**Tech Stack:** Rust (tokio, tokio-postgres), Vue 3 + naive-ui + Vite, pnpm.

---

## Reference: current behavior

- `rs-back/src/master.rs` — `Worker::new` assigns `id = uuid`, `name = "worker"`;
  `ping()` updates `name` from the pong; `sync_to_db` upserts by `id`; `cleanup()`
  deletes the row on disconnect; `start()`'s expirer deletes `last_ping < now-30`.
- `rs-back/src/routes.rs:get_workers` — `SELECT ... FROM workers ORDER BY last_ping DESC`.
- `src/components/Workers.vue` — polls `/api/workers` every 5s, renders per-worker
  cards; calls `worker.avg_det_time.toFixed(2)` (throws if null).
- `package.json` — frontend deps.

All paths relative to repo root `/home/lihe07/Disk1/codes/swift-lite`.

---

## Task 1: Worker identity/lifecycle helpers (pure, unit-tested)

Add testable constants and pure functions to `master.rs` for the online cutoff and
the stats-seeding rule, so the lifecycle logic is verified independently of the DB.

**Files:**
- Modify: `rs-back/src/master.rs` (add constants + helpers + tests)

- [ ] **Step 1: Write the failing tests**

Add near the top of `rs-back/src/master.rs`, after the `use` block:
```rust
/// A worker is "online" if its last_ping is within this many seconds of now.
pub const ONLINE_SECS: i32 = 60;
/// A worker row is purged after this many seconds without any ping (1 week).
pub const PURGE_SECS: i32 = 604_800;

/// last_ping cutoff (inclusive lower bound) for a worker to count as online.
pub fn online_cutoff(now: i32) -> i32 {
    now - ONLINE_SECS
}

/// last_ping cutoff (exclusive upper bound) below which a row is purged.
pub fn purge_cutoff(now: i32) -> i32 {
    now - PURGE_SECS
}

/// Seed (tasks_done, avg_det_time, connected_at) for a connecting worker.
/// If a prior row exists, accumulate from it (preserving first-seen connected_at);
/// otherwise start fresh anchored at `now`.
pub fn seed_identity(existing: Option<(i64, f64, i32)>, now: i32) -> (i64, f64, i32) {
    match existing {
        Some((tasks_done, avg, connected_at)) => (tasks_done, avg, connected_at),
        None => (0, 0.0, now),
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn cutoffs() {
        assert_eq!(online_cutoff(1000), 1000 - 60);
        assert_eq!(purge_cutoff(1_000_000), 1_000_000 - 604_800);
    }

    #[test]
    fn seed_fresh_when_no_prior_row() {
        assert_eq!(seed_identity(None, 1234), (0, 0.0, 1234));
    }

    #[test]
    fn seed_accumulates_and_preserves_first_connection() {
        // prior row: 7 tasks, avg 1.5s, first connected at t=500; reconnecting at t=9000
        let (td, avg, ca) = seed_identity(Some((7, 1.5, 500)), 9000);
        assert_eq!(td, 7);
        assert_eq!(avg, 1.5);
        assert_eq!(ca, 500); // first-seen preserved, NOT reset to 9000
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd rs-back && cargo test helper_tests`
Expected: compile error / FAIL until the functions exist (they're added in Step 1, so this step confirms they compile and pass once added).

- [ ] **Step 3: (implementation is in Step 1)**

The constants/functions above are the implementation.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd rs-back && cargo test helper_tests`
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add rs-back/src/master.rs
git commit -m "feat: add worker online/purge/seed helpers"
```

---

## Task 2: Refactor Worker to key by name + accumulate stats

Replace the UUID identity with the worker name, seed stats from the DB on connect,
stop deleting the row on disconnect, and switch the expirer to a 1-week purge.

**Files:**
- Modify: `rs-back/src/master.rs`

- [ ] **Step 1: Add a `load_identity` method and call it after the first ping**

In `impl Worker`, add this method (place it right after `ping`):
```rust
    /// After the first ping (name known), key the row by name and seed stats
    /// from any existing row so they accumulate across reconnects.
    async fn load_identity(&mut self) -> anyhow::Result<()> {
        self.id = self.name.clone();
        let existing = {
            let c = self.pool.get().await?;
            let rows = c
                .query(
                    "SELECT tasks_done, avg_det_time, connected_at FROM workers WHERE id = $1",
                    &[&self.id],
                )
                .await?;
            rows.first().map(|r| {
                (
                    r.try_get::<_, Option<i32>>("tasks_done").ok().flatten().unwrap_or(0) as i64,
                    r.try_get::<_, Option<f64>>("avg_det_time").ok().flatten().unwrap_or(0.0),
                    r.try_get::<_, Option<i32>>("connected_at").ok().flatten().unwrap_or(self.connected_at),
                )
            })
        };
        let (tasks_done, avg, connected_at) = seed_identity(existing, self.connected_at);
        self.tasks_done = tasks_done;
        self.avg_det_time = avg;
        self.connected_at = connected_at;
        Ok(())
    }
```

- [ ] **Step 2: Call `load_identity` at the start of `run`**

In `Worker::run`, change the opening:
```rust
    async fn run(&mut self) -> anyhow::Result<()> {
        if !self.ping().await {
            anyhow::bail!("failed initial ping");
        }
        self.load_identity().await?;
        loop {
```
(Insert the `self.load_identity().await?;` line between the initial ping check and `loop {`.)

- [ ] **Step 3: Stop deleting the row on disconnect**

Replace the body of `cleanup` so it no longer deletes (stats must survive):
```rust
    async fn cleanup(&self) {
        // Intentionally does NOT delete the worker row: stats persist across
        // reconnects and are only removed by the 1-week purge in `start`.
    }
```

- [ ] **Step 4: Switch the expirer to a 1-week purge**

In `start`, change the expirer's DELETE from the 30s window to the purge cutoff:
```rust
                if let Ok(c) = pool.get().await {
                    let _ = c
                        .execute(
                            "DELETE FROM workers WHERE last_ping < $1",
                            &[&purge_cutoff(db::now())],
                        )
                        .await;
                }
```

- [ ] **Step 5: Note the duplicate-name edge case**

Add this comment above the `struct Worker` definition:
```rust
// NOTE: the worker row is keyed by the worker-supplied name (`id == name`).
// Two simultaneous connections sharing a name will share/clobber one row; names
// are expected to be unique per worker, so this is accepted.
```

- [ ] **Step 6: Verify it builds and all backend tests pass**

Run: `cd rs-back && cargo build && cargo test -- --test-threads=1`
Expected: compiles; all tests (helpers + existing) pass.

- [ ] **Step 7: Commit**

```bash
git add rs-back/src/master.rs
git commit -m "feat: persist worker stats by name across reconnects"
```

---

## Task 3: Filter `/api/workers` to online workers only

**Files:**
- Modify: `rs-back/src/routes.rs:get_workers`

- [ ] **Step 1: Update the query to filter by the online cutoff**

Replace the `get_workers` query call:
```rust
async fn get_workers(State(s): State<AppState>) -> Result<Json<Value>, AppError> {
    let c = s.pool.get().await?;
    let cutoff = crate::master::online_cutoff(db::now());
    let rows = c
        .query(
            "SELECT id, name, connected_at, avg_det_time, last_ping, tasks_done FROM workers WHERE last_ping >= $1 ORDER BY last_ping DESC",
            &[&cutoff],
        )
        .await?;
```
(Leave the row→JSON mapping and the returned `{ "data": [...] }` shape unchanged.)

- [ ] **Step 2: Verify build + the no-DB router tests still pass**

Run: `cd rs-back && cargo build && cargo test routes:: -- --test-threads=1`
Expected: compiles; the 3 route tests pass.

- [ ] **Step 3: Commit**

```bash
git add rs-back/src/routes.rs
git commit -m "feat: GET /api/workers returns online workers only"
```

---

## Task 4: Bump frontend dependencies to latest

**Files:**
- Modify: `package.json`
- Regenerate: `pnpm-lock.yaml`

- [ ] **Step 1: Update `package.json` versions**

Set `dependencies` and `devDependencies` to:
```json
  "dependencies": {
    "lodash": "^4.18.1",
    "vue": "^3.5.38",
    "vue-confetti-explosion": "^1.0.2",
    "vue-router": "^5.1.0"
  },
  "devDependencies": {
    "@vicons/fluent": "^0.13.0",
    "@vitejs/plugin-vue": "^6.0.7",
    "naive-ui": "^2.44.1",
    "openseadragon": "^6.0.2",
    "vite": "^8.0.16"
  }
```

- [ ] **Step 2: Reinstall**

Run: `pnpm install`
Expected: resolves and writes an updated `pnpm-lock.yaml` with no peer-dependency errors. If a peer error appears, read it and adjust the offending version.

- [ ] **Step 3: Verify production build**

Run: `pnpm build`
Expected: `vite build` completes and writes `dist/`. If the vite 8 major introduced a config break, fix `vite.config.js` accordingly (the current config only uses `@vitejs/plugin-vue` and a `/api` proxy, both stable across vite 8).

- [ ] **Step 4: Verify dev server boots**

Run: `timeout 8 pnpm dev || true`
Expected: Vite prints the local dev URL with no startup error. (The `timeout` ends it; we only need to confirm it starts.)

- [ ] **Step 5: Commit**

```bash
git add package.json pnpm-lock.yaml
git commit -m "chore: bump frontend deps to latest"
```

---

## Task 5: Redesign the per-worker stats cards

Rework `Workers.vue`: cleaner per-worker cards with a live indicator, first-connection
and last-seen relative times, tasks done, average detection time, and a derived
throughput. Guard `avg_det_time` against null.

**Files:**
- Modify: `src/components/Workers.vue`

- [ ] **Step 1: Replace `Workers.vue`**

```vue
<script setup>
import { ref, onMounted, computed } from "vue";
import { NGrid, NGridItem, NCard, NTime, NStatistic, NTag, NSpace } from "naive-ui";

const workers = ref({ data: [] });
const now = ref(Date.now());

async function refresh() {
  try {
    workers.value = await fetch("/api/workers").then((r) => r.json());
    now.value = Date.now();
  } catch (e) {
    console.error("Error fetching workers:", e);
  }
}

const list = computed(() => (workers.value && workers.value.data) || []);

/** tasks per hour over the worker's uptime (connected_at -> last_ping). */
function throughput(w) {
  const uptime = Math.max(1, (w.last_ping || 0) - (w.connected_at || 0));
  return ((w.tasks_done || 0) / uptime) * 3600;
}

onMounted(() => {
  refresh();
  const timer = setInterval(refresh, 5000);
  return () => clearInterval(timer);
});
</script>

<template>
  <div v-if="list.length === 0" style="text-align: center">
    <h3>🔥 无计算节点，服务中止！</h3>
  </div>

  <n-grid v-else cols="1 600:2" x-gap="10" y-gap="10">
    <n-grid-item v-for="worker in list" :key="worker.id">
      <n-card size="small">
        <template #header>
          <n-space align="center" :size="8">
            <n-tag type="success" size="small" round>● 在线</n-tag>
            <span>{{ worker.name }}</span>
          </n-space>
        </template>

        <n-grid cols="2" y-gap="12" x-gap="8">
          <n-grid-item>
            <n-statistic label="首次连接">
              <n-time :time="worker.connected_at" :to="now / 1000" unix type="relative" />
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="最近活动">
              <n-time :time="worker.last_ping" :to="now / 1000" unix type="relative" />
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="处理任务量">
              {{ worker.tasks_done ?? 0 }}
            </n-statistic>
          </n-grid-item>
          <n-grid-item>
            <n-statistic label="平均耗时">
              {{ (worker.avg_det_time ?? 0).toFixed(2) }} 秒
            </n-statistic>
          </n-grid-item>
          <n-grid-item :span="2">
            <n-statistic label="吞吐量">
              {{ throughput(worker).toFixed(1) }} 张 / 小时
            </n-statistic>
          </n-grid-item>
        </n-grid>
      </n-card>
    </n-grid-item>
  </n-grid>
</template>
```

- [ ] **Step 2: Verify the build includes the new component**

Run: `pnpm build`
Expected: build succeeds (the component compiles; all imported naive-ui symbols
`NGrid,NGridItem,NCard,NTime,NStatistic,NTag,NSpace` exist in naive-ui 2.44).

- [ ] **Step 3: Commit**

```bash
git add src/components/Workers.vue
git commit -m "feat: redesign worker stats cards"
```

---

## Task 6: Manual verification (checkpoint)

- [ ] **Step 1: With a Postgres + `rs-back` running**, connect a worker, let it
  finish a detection, kill the worker process, then restart it with the **same
  name**. Confirm via `GET /api/workers` (or the home page card) that `tasks_done`
  and `首次连接` (connected_at) carry over rather than resetting.

- [ ] **Step 2:** Confirm a worker that stays disconnected drops off the
  `/api/workers` list within ~60s but its row remains in the `workers` table
  (`SELECT * FROM workers`) until the 1-week purge.

- [ ] **Step 3:** Load the home page and confirm the redesigned cards render with
  the online tag, relative times, task count, avg time, and throughput.

- [ ] **Step 4: Commit any fixes found during verification.**

---

## Self-Review Notes

- **Spec coverage:** identity-by-name + seed/accumulate (Task 2), online cutoff
  60s + 1-week purge + helpers (Tasks 1–2), online-only `/api/workers` (Task 3),
  deps bump (Task 4), card redesign + null guard (Task 5), reconnect/offline
  verification (Task 6). All spec sections map to a task.
- **No schema change:** `id` repurposed as name; `ensure_tables` untouched.
- **Type consistency:** `seed_identity(Option<(i64,f64,i32)>, i32) -> (i64,f64,i32)`
  is used identically in its test and in `load_identity`; `online_cutoff`/
  `purge_cutoff` take and return `i32` (matching the INTEGER columns and `db::now()`).
- **Known accepted deviation:** duplicate worker names share one row (documented).
