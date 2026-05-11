# Journal — 2026-05-11 — Phase 3a plan doc (Lark + team sync + handoff)

## PR: docs(plans) — phase 3a — Lark Bitable + team sync + handoff

**Branch:** `docs/phase-3a-lark-team-sync-plan` (cherry-picked dari
`feat/phase-2b-interactive`) **Author:** handokobeni **PR:** #18

### Summary

Plan document untuk **Phase 3a — Lark Bitable Sync + Team Activity + Handoff**,
fase setelah Phase 2b (interactive surfaces). Goal: ubah Ansambel dari single-PC
orchestrator jadi per-engineer client dari shared task source-of-truth di Lark
Bitable, **tanpa** mengubah model per-PC desktop install — no headless daemon,
no central server. Bitable jadi satu-satunya shared state.

Ini PR docs-only (dua commit, satu doc file 667+ baris). Tidak ada code change,
tidak nyentuh CI gates.

### Commits

```
77e5b79 docs(plans): phase 3a — Lark Bitable + team sync + handoff
2369252 docs(plans): phase 3a — strict per-repo scope (hard hide, not soft filter)
```

Dua-duanya cherry-pick dari `feat/phase-2b-interactive` (di mana doc-nya
authored aslinya) ke branch terpisah supaya bisa di-review/merge independent
dari feature Phase 2b. Branch base: `origin/main`.

### Konteks workflow yang dipecahkan

User cerita kebutuhan internal: tim engineer berbagi task di Lark chatbot di VM.
Workflow saat ini punya banyak friction — engineer harus remote-in ke VM untuk
lihat progress, gak ada konteks per-task, dan handoff antar engineer kalau ganti
shift = manual paste status di chat.

Phase 3a coba jawab itu dengan menggeser source-of-truth task ke Lark Bitable
yang bisa diakses dari Ansambel di tiap PC engineer. Visibility ke task orang
lain difilter ketat: tiap engineer cuma lihat task untuk repo yang mereka punya
secara lokal — strict per-repo scope.

### 8 sub-phase (~6 minggu solo)

| Sub-phase | Fokus                                                    |
| --------- | -------------------------------------------------------- |
| 3a-1      | TaskProvider trait abstraction (lokal + Lark)            |
| 3a-2      | Lark Bitable schema + read/write client                  |
| 3a-3      | Sync engine: polling + change-set + conflict resolution  |
| 3a-4      | Team Activity sidebar — strict per-repo filter           |
| 3a-5      | Per-workspace shared-with-team toggle                    |
| 3a-6      | Workspace metadata fields (last activity, message count) |
| 3a-7      | Settings UI (Lark connection, sync interval, scope)      |
| 3a-8      | Manual handoff: bundle conversation + WIP changes        |

Bitable schema final: 16 fields termasuk `repo_id`, `ansambel_status`,
`last_activity_at`, `last_message_preview`, `pr_url`, `private`,
`handoff_target`, `handoff_bundle`.

### Scope decisions yang diambil

1. **Strict per-repo scope (UX-only enforcement).** Engineer yang gak punya
   `project-x` secara lokal gak akan lihat _apa-apa_ dari `project-x` — bukan
   soft filter, hard hide. Bitable membership memberi "trust boundary" pertama;
   per-repo filter memberi yang kedua. Tradeoff: bukan cryptographic isolation
   (risk #4 di plan doc) — engineer yang sengaja clone repo orang lain bisa
   muncul di pool. Untuk tim internal, ini cukup.
2. **Manual handoff (bukan scheduled auto-run).** User explicit minta skip
   scheduled auto-run. Handoff cuma jalan kalau user trigger tombol. Reasoning:
   scheduled run butuh PC nyala terus + wake lock per-OS + autoresume Claude —
   masing-masing besar sendiri. Di-defer ke Phase 7-mini.
3. **Auto-PR allowed (tapi never main/master).** Handoff bisa otomatis bikin PR
   ke remote tapi gak akan push ke protected branch apapun. Guardrail di auto-PR
   helper.
4. **Bundle format**: tar.gz dari `messages.jsonl` + `wip.patch` +
   `untracked.tar` + `todos.json` + `state.json`. Upload ke Lark attachment. WIP
   commit di-push ke branch `handoff/<workspace>` sebagai snapshot. Penerima
   download bundle + apply via `git reset --soft` + `git apply`.

### Out of scope (di-defer eksplisit)

- **Jira / multi-provider abstraction** → Phase 3b (Bitable dulu, Jira nanti,
  arsitektur sudah disiapin via trait di 3a-1)
- **Scheduled auto-run** → Phase 7-mini (kalau ever)
- **Headless daemon mode** → Phase 9 (originally sketsa ditolak user karena
  terlalu kompleks)
- **Auto-PR dengan safeguards** → Phase 7-mini (bareng scheduled run kalau
  dibikin)

### Architecture clarification yang sempat dirumusin ulang

Awal draft plan-nya saya tulis sebagai "soft filter" — engineer tetap lihat
metadata task orang lain tapi gak bisa buka. User tegasin: **strict** — engineer
tanpa repo gak boleh lihat _apa-apa_. Plan doc direvisi di commit kedua untuk:

- 3a-4 (Team Activity sidebar): hapus "All repos" filter, hapus "Add this repo"
  modal untuk non-members, empty state kalau zero overlap
- 3a-7 (Settings): hapus repo filter dropdown — scope sudah hard
- 3a-8 (Handoff): handoff picker cuma list engineer dengan repo overlap
- Risk #4 ditambahin: UX-only-vs-server-enforced trade-off

### Test plan (di PR description)

- [ ] Skim plan doc end-to-end — sub-phase split + cadence realistis
- [ ] Strict-scope decision sesuai discussion (UX-only, no "Add this repo"
      prompt)
- [ ] Bitable 16-field schema cukup untuk activity + handoff flow
- [ ] 9 risk cover semua kasus, terutama #4 (UX enforcement)

### Lessons

- **Plan doc sebagai produk diskusi**: PR ini ujungnya adalah ~30 ronde Q&A
  dengan user untuk mempertajam scope. Doc finalnya bukan "blueprint sudah
  jelas" tapi "alignment yang sudah jadi artifact tertulis". Tanpa doc, hari
  pertama implementasi bakal ulang diskusi yang sama.
- **Dua commit beda hari**: initial plan + strict-scope revision di-pisah commit
  karena revisinya substansial (mengubah ~3 sub-phase). Reviewer bisa baca
  delta-nya tanpa harus diff full 667 baris.
- **Branch dipisah dari Phase 2b**: meski awalnya doc ada di branch
  feat/phase-2b, cherry-pick ke branch docs-only bikin reviewer cuma fokus ke
  perubahan doc tanpa harus skip code reviewing Phase 2b.
