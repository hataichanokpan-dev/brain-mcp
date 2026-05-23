# Knowledge Engine Blueprint v1.0

**Codename:** `brain-mcp`
**Status:** Draft for Review
**Audience:** Self (operator) + future implementer
**Deployment target:** Oracle Cloud Always Free Tier (ARM Ampere)
**Clients:** Claude Desktop, Claude Code, Codex CLI, future IDE agents via MCP/ACP

---

## 0. Purpose & Scope

### 0.1 What this is
ระบบ "สมองกลาง" สำหรับ Claude Desktop + Claude Code + agent อื่นๆ ที่รองรับ MCP โดยทำหน้าที่:
- จดจำ **กฎและสไตล์การทำงาน** ของผู้ใช้ (Profile)
- สะสม **ความรู้แบบ declarative** เกี่ยวกับ topic/concept/project (Semantic)
- สะสม **workflow ที่ verified แล้ว** ที่ agent นำไป execute ได้ (Procedural)
- ค้นได้เร็ว แม่นยำ รองรับภาษาไทย scale ถึง 10,000+ pages
- Auditable, recoverable, no silent writes

### 0.2 What this is NOT (out of scope v1)
- Episodic memory (chat history recall) — ถ้าจำเป็นค่อยเพิ่มเฟส 2
- Working memory / scratchpad — เป็น harness concern ไม่ใช่ MCP
- Real-time collaboration / multi-user — single-operator design
- Mobile/offline sync — Oracle VM เป็น single source of truth
- Vector search ของรูปภาพ/audio — text only

### 0.3 Design philosophy
1. **Boring tech** — Markdown + Git + Rust binary + Qdrant ทุกอย่างพิสูจน์แล้ว
2. **Reversible everything** — git revert กลับได้ทุก state
3. **No silent writes** — ทุกการเขียนผ่าน propose/commit gate
4. **Verification before autonomy** — ระบบต้อง verify ผลของตัวเองได้ก่อนจะอนุญาตให้ทำงาน auto
5. **Separate storage from policy** — store แต่ละตัวจัดการ data, policy อยู่ใน MCP wrapper

---

## 1. Architecture at a Glance

```
┌─────────────────────────────────────────────────────────────────┐
│  Clients (MCP/ACP)                                              │
│  Claude Desktop  │  Claude Code  │  Codex CLI  │  Zed/Cursor    │
└────────┬──────────────┬──────────────┬────────────┬─────────────┘
         │              │              │            │
         └──────────────┴──────┬───────┴────────────┘
                               │ MCP over stdio/HTTP
                ┌──────────────▼──────────────┐
                │   brain-mcp (gateway)       │
                │   - Permission boundary     │
                │   - Propose/Commit gate     │
                │   - Audit logger            │
                │   - Hybrid retrieval orch.  │
                └──┬──────────┬──────────┬───┘
                   │          │          │
        ┌──────────▼──┐  ┌────▼─────┐  ┌─▼──────────────┐
        │ llm-wiki    │  │ Qdrant   │  │ Embedding svc  │
        │ engine      │  │ (vector) │  │ (BGE-M3)       │
        │ - Tantivy   │  │          │  │ + Reranker     │
        │ - Git       │  │          │  │ (BGE-rr-v2-m3) │
        │ - Petgraph  │  │          │  │                │
        └──────┬──────┘  └────┬─────┘  └────────────────┘
               │              │
               └──────┬───────┘
                      │
              ┌───────▼────────┐
              │ Filesystem     │
              │ ~/wikis/brain  │
              │   profile/     │
              │   semantic/    │
              │   procedural/  │
              │   .git/        │
              └────────────────┘
```

**หลักการ:** llm-wiki = source of truth + BM25 + git. Qdrant = vector index (derived). brain-mcp = policy + orchestration + permission layer. Filesystem markdown เป็น canonical — ทุก index อื่นๆ rebuild ได้จากนี่

---

## 2. The Three Stores

### 2.1 Profile (Constitution)

**Purpose:** กฎ ตัวตน สไตล์การทำงาน ที่โหลดทุก session

**Examples:**
- "ห้ามใช้ emoji ในโค้ด commit message"
- "ภาษาที่ใช้ตอบ: ไทย-อังกฤษผสม technical term เป็นอังกฤษ"
- "ก่อน commit ต้องรัน lint และ test"
- "Stack หลัก: TypeScript + Bun + ESM"

**Schema (frontmatter):**
```yaml
---
type: profile
section: rules | identity | style | stack | constraints
priority: hard | soft           # hard = ห้ามฝ่าฝืน
status: active | superseded
supersedes: profile/rules/old-id  # ถ้ามาแทนของเก่า
created: 2026-05-23
last_verified: 2026-05-23
---
```

**Lifecycle:**
- **NO decay** — แต่มี supersession
- Versioned ผ่าน git
- โหลดทุก session ผ่าน `profile_get` (cache 5 นาที)
- การเปลี่ยน Profile ต้องผ่าน `profile_propose_update` + confirm diff

**Storage layout:**
```
profile/
  identity.md
  hard-rules.md
  soft-preferences.md
  style-guide.md
  stack.md
  constraints.md
```

**Size budget:** 1-3 KB total — ต้องโหลดได้ใน <100ms

---

### 2.2 Semantic (Concept Wiki)

**Purpose:** Declarative knowledge — "X คืออะไร", "ทำไม Y สำคัญ", "Z สัมพันธ์กับ W ยังไง"

**Sub-types:**
- `concept` — แนวคิด/หลักการ (เช่น "Reciprocal Rank Fusion")
- `entity` — คน/องค์กร/product (เช่น "Anthropic", "Qdrant")
- `source` — สรุปจาก paper/article/book ที่ ingest มา
- `project` — context ของ project ที่กำลังทำ
- `decision` — architectural decision (ADR-style)

**Schema:**
```yaml
---
type: concept | entity | source | project | decision
title: Reciprocal Rank Fusion
status: active | draft | stale | contested
confidence: 0.0-1.0
tags: [retrieval, ranking, fusion]
sources:
  - sources/cormack-2009-rrf
related:
  - concepts/hybrid-search
  - concepts/bm25
last_lint: 2026-05-23
content_hash: sha256:...        # ใช้ detect change → re-embed
embedding_version: bge-m3-v1    # ใช้ตอน migrate model
---
```

**Lifecycle:**
- **ไม่ decay เป็น hard delete** — แต่ flag `stale` ถ้า source ทั้งหมดเก่ากว่า 12 เดือน และไม่มี backlink ใน 6 เดือน
- **Contradiction** → flag `contested` ให้ human review ห้าม auto-resolve
- **Compounding** — แต่ละ source ingest อัปเดต semantic pages ที่เกี่ยวข้อง (Karpathy DKR pattern)

**Storage layout:**
```
semantic/
  concepts/
    reciprocal-rank-fusion.md
    hybrid-search.md
    ...
  entities/
    qdrant.md
    anthropic.md
  sources/
    cormack-2009-rrf.md
  projects/
    brain-mcp.md
  decisions/
    adr-001-qdrant.md
```

---

### 2.3 Procedural (Runbook)

**Purpose:** Executable workflows — "ทำ X ยังไง" + verification

**Critical difference from Semantic:** Procedural ต้องมี **verification step** ทุกอัน ถ้าไม่มีคือยังไม่ promote

**Schema:**
```yaml
---
type: procedure
title: Deploy brain-mcp to Oracle VM
status: verified | draft | deprecated
verified_count: 5                 # รันสำเร็จกี่ครั้ง
last_verified: 2026-05-23
last_failed: null
failure_count: 0
inputs:
  - name: vm_ip
    type: ipv4
  - name: ssh_key_path
    type: filepath
outputs:
  - mcp_endpoint_url
preconditions:
  - VM running Ubuntu 22.04 ARM
  - SSH access established
postconditions:
  - MCP server responds on :8765
tags: [deployment, mcp, oracle]
related_procedures:
  - procedural/qdrant-setup
estimated_duration: 15min
risk_level: low | medium | high
---

## Steps
1. ...
2. ...

## Verification
- [ ] curl http://{vm_ip}:8765/health → expect 200
- [ ] mcp_list_tools → expect >= 10 tools
- [ ] write test profile → read back → bytewise equal

## Failure Modes
- If step 3 fails with "permission denied" → ...
- If verification step 2 returns <10 → re-run `mcp_register`

## Rollback
1. ...
```

**Lifecycle:**
- **Promotion:** draft → verified ต้องผ่าน `verification` block สำเร็จ ≥3 ครั้ง
- **Demotion:** ถ้า `failure_count` ใน 30 วัน ≥2 → flag `deprecated`, ห้าม recommend
- **NO time-based decay** — procedure เก่าที่ยัง work อยู่ คือทรัพย์สิน

**Storage layout:**
```
procedural/
  deployment/
    deploy-brain-mcp.md
    setup-qdrant.md
  development/
    create-new-mcp-tool.md
  troubleshooting/
    qdrant-oom.md
```

---

### 2.4 The Acid Test: Semantic vs Procedural

ถ้าตอบ "ใช่" ทั้ง 3 ข้อ → Procedural ไม่ครบ → Semantic

| Question | Procedural | Semantic |
|---|---|---|
| Junior copy-paste แล้ว execute ได้เลย? | ✅ | ❌ |
| มี verification step ที่ pass/fail ชัดเจน? | ✅ | ❌ |
| คำตอบของ "ทำสิ่งนี้สำเร็จมั้ย" เป็น boolean? | ✅ | ❌ |

**Edge case:** "การ deploy คืออะไร" → Semantic. "deploy brain-mcp ยังไง" → Procedural. คอนเซปต์เดียวกันแต่ schema และ lifecycle ต่างกัน

---

## 3. Storage Architecture

### 3.1 Canonical Layer: Markdown + Git

**Why:** human-readable, diffable, portable, git ให้ audit trail ฟรี
**Path:** `~/wikis/brain/` (single git repo)
**Backup:** daily push to private GitHub/Gitea
**Conflict resolution:** none needed (single writer per session via lock file)

### 3.2 Keyword Index: Tantivy (via llm-wiki)

**Role:** keyword precision, BM25 ranking
**Used for:** exact term match, tag/type/status facets
**Update:** real-time at `wiki_ingest`
**Stays in llm-wiki engine** — ไม่แตะ

### 3.3 Vector Index: Qdrant

**Why Qdrant (vs alternatives):**

| Option | Pros | Cons | Decision |
|---|---|---|---|
| **Qdrant** | Rust, fast, payload filter, ARM build, embedded mode | ต้องรัน service เพิ่ม | ✅ Selected |
| pgvector | Single Postgres = ลด moving parts | ช้ากว่า, filter ดี้กว่า, ไม่มี HNSW tuning | Honorable mention |
| Milvus | Scale มหาศาล | Overkill, ARM support เพิ่งจะดี | ❌ |
| Weaviate | มี module เยอะ | Heavy, JVM-ish footprint | ❌ |
| LanceDB | Embedded, file-based | Ecosystem ใหม่กว่า | ❌ (revisit ใน v2) |
| Chroma | ง่าย, Python native | Scale issue เกิน 1M vectors | ❌ |

**Collection design:**

```
Collection: brain_semantic_chunks
  vectors:
    dense: {size: 1024, distance: Cosine}          # BGE-M3 dense
    sparse: {modifier: idf}                          # BGE-M3 sparse
  payload:
    page_id: keyword (indexed)
    page_path: keyword
    chunk_idx: integer
    chunk_text: text
    type: keyword (indexed)        # concept|entity|source|...
    tags: keyword[] (indexed)
    status: keyword (indexed)
    confidence: float
    content_hash: keyword
    embedding_version: keyword
    created: datetime
    last_modified: datetime
```

**Index params (HNSW):**
- `m: 16` (graph connections per node — default OK)
- `ef_construct: 200` (higher = better quality, slower build)
- `ef: 128` (query-time, balance speed/recall)
- `quantization: scalar (int8)` — ลด RAM ~75% เกือบไม่เสีย recall

**Separate collection:** `brain_procedural_chunks` (same schema) — แยกเพราะ retrieval policy ต่างกัน (procedural ต้องกรอง `status: verified` เสมอ)

**Profile ไม่ใส่ใน Qdrant** — เล็กพอจะ scan ตรง, latency ต่ำกว่า

### 3.4 Graph Layer: Decision Tree

**v1 (now): petgraph (in-memory ใน llm-wiki)**
- ดีพอสำหรับ <5,000 nodes
- เร็ว (in-memory), reload จาก markdown frontmatter ตอน startup
- Edge types จาก frontmatter: `sources`, `related`, `supersedes`, `related_procedures`

**v2 trigger (เมื่อไหร่ upgrade):**
- จำนวน pages > 5,000
- Graph query latency > 200ms
- ต้อง multi-hop reasoning เกิน 3 hops
- ต้อง persist graph state ระหว่าง restart

**v2 choice: KuzuDB**
- Embedded (single file like SQLite)
- Cypher query support
- ARM build OK
- ไม่ต้อง Neo4j (heavy, JVM, overkill)

**ไม่ใช้ Neo4j:** ใช้ RAM เยอะ, JVM, license complexity, ขนาดข้อมูลไม่คุ้ม

---

## 4. Embedding Strategy

### 4.1 Model Choice: BGE-M3

**Why BGE-M3:**
- รองรับไทย native (อยู่ใน 100+ languages ที่ฝึก)
- Context 8,192 tokens (ส่วนใหญ่รุ่นอื่นได้ 512)
- Multi-functional: ออก **dense + sparse + ColBERT** ในโมเดลเดียว
- ขนาด 568M params, quantize int8 → ~600MB RAM
- ARM compatible (PyTorch CPU หรือผ่าน llama.cpp)
- Apache 2.0 license

**Alternatives considered:**

| Model | Thai support | Self-host | Decision |
|---|---|---|---|
| **BGE-M3** | ✅ Native | ✅ | ✅ Selected |
| multilingual-e5-large | ✅ | ✅ | Backup option |
| jina-embeddings-v3 | ✅ | ✅ | Strong, but Matryoshka adds complexity |
| paraphrase-multilingual-mpnet-base-v2 | ⚠️ Limited Thai | ✅ | ❌ |
| OpenAI text-embedding-3-large | ✅ | ❌ (API only) | Backup if self-host พัง |
| Cohere embed-multilingual-v3 | ✅ | ❌ | ❌ (vendor lock) |
| WangchanBERTa | ✅ Thai-only | ✅ | ❌ (ไม่ multilingual) |

**Migration path:** ถ้าจะเปลี่ยน model — payload field `embedding_version` ใช้ตรวจสอบ ไม่ต้อง re-embed ทั้งหมด รัน reembed batch สำหรับ pages ที่ version ไม่ตรง

### 4.2 Chunking Strategy

- **Chunk size:** 512 tokens (ไม่ใช่ words — ใช้ BGE-M3 tokenizer)
- **Overlap:** 64 tokens
- **Boundary respect:** ไม่ตัดกลาง heading/code block — ใช้ structural chunking
  1. Split by `##` heading ก่อน
  2. ถ้า section > 512 → split by paragraph
  3. ถ้า paragraph > 512 → sliding window 512/64

- **Metadata preservation:** ทุก chunk เก็บ `page_id` + `chunk_idx` + frontmatter copy ไว้ใน payload
- **Frontmatter ไม่เข้าเป็น chunk** — ใส่เป็น payload filter

### 4.3 Multi-Vector Approach

BGE-M3 ให้ output 3 แบบในการ embed ครั้งเดียว:

1. **Dense (1024 dim)** — semantic similarity, ใช้เป็น primary
2. **Sparse (BGE-M3 sparse output)** — lexical match แบบ neural, complement กับ BM25 ของ tantivy
3. **ColBERT-style (optional v2)** — late interaction, accuracy สูงสุดแต่ slow

**v1:** dense + sparse (ใน Qdrant collection เดียว, named vectors)
**v2:** เพิ่ม ColBERT ถ้า recall ยังไม่พอ

### 4.4 Embedding Refresh Policy

- ที่ ingest: ถ้า `content_hash` ในใหม่ != ใน Qdrant → re-embed chunk นั้น
- ที่ model upgrade: ตรวจ `embedding_version`, re-embed pages ที่ version ไม่ตรง batch ละ 100
- ลบ chunk ที่ orphan (มี payload แต่ markdown ไม่มี) ใน lint pass

---

## 5. Retrieval Pipeline

### 5.1 Stage 1: Parallel Recall (target <100ms)

ยิง 3 query ขนานกัน:

1. **BM25** ผ่าน tantivy → top-30
2. **Dense vector** ผ่าน Qdrant (cosine) → top-30
3. **Sparse vector** ผ่าน Qdrant (dot product) → top-30

Apply payload filters:
- `status != stale` (เว้นแต่ค้น stale โดยตรง)
- `status != contested` หรือ surface พร้อม warning
- Type filter ถ้า client ระบุ

### 5.2 Stage 2: Fusion (target <10ms)

**Reciprocal Rank Fusion (RRF)** with k=60:
```
RRF_score(d) = Σ 1 / (k + rank_i(d))
```
สำหรับทุก retriever i ที่ document d ปรากฏ

Output: top-50 หลัง fuse

### 5.3 Stage 3: Cross-Encoder Rerank (target <200ms)

**Model:** BGE-reranker-v2-m3 (multilingual, ~568M, รองรับไทย)
**Input:** query + top-50 chunks
**Output:** top-10 reranked by relevance score
**Threshold:** ตัด score < 0.3 ออก แม้จะอยู่ใน top

### 5.4 Stage 4: Context Assembly (target <50ms)

- รวบ chunk เป็น page-level (group by `page_id`)
- ถ้า client ขอ `with_backlinks: true` → ดึง backlinks 1 hop จาก petgraph
- Return ทั้ง full content + relevance score + provenance

**Total latency budget: <500ms p95 ที่ 1,000 pages, <800ms ที่ 10,000 pages**

---

## 6. MCP Tool Surface

### 6.1 Read Tier (no side effects, ใช้บ่อย)

| Tool | Args | Returns |
|---|---|---|
| `profile_get` | section? | Full Profile or section |
| `semantic_search` | query, top_k=10, filters? | Ranked chunks + provenance |
| `semantic_get` | page_id, with_backlinks? | Full page content |
| `procedural_find` | intent (NL), context? | Top matching runbooks |
| `procedural_get` | proc_id | Full runbook + status |
| `graph_neighbors` | page_id, depth=1, edge_types? | Related pages by type |
| `audit_history` | path, limit=10 | Git commits for page |

### 6.2 Propose Tier (write draft, ต้อง confirm)

| Tool | Args | Returns |
|---|---|---|
| `memory_propose` | type, title, content, frontmatter | draft_id + diff preview |
| `profile_propose_update` | section, change | draft_id + diff |
| `procedure_propose` | full runbook YAML+MD | draft_id + lint result |

Drafts เก็บใน `~/wikis/brain/.drafts/` — ไม่ commit ลง git จนกว่าจะ promote

### 6.3 Commit Tier (apply draft)

| Tool | Args | Returns |
|---|---|---|
| `memory_commit` | draft_id, confirm_token | committed page_id + git_sha |
| `procedure_promote` | proc_id, verification_evidence | new status + git_sha |
| `procedure_demote` | proc_id, failure_evidence | new status + git_sha |

**Confirm token:** สร้างตอน propose, expire ใน 10 นาที — บังคับ explicit confirmation

### 6.4 Maintenance Tier (run via skill, not chat)

| Tool | Args | Returns |
|---|---|---|
| `consolidate_run` | dry_run=true, scope? | Report of proposed changes |
| `consolidate_apply` | run_id, items[] | Applied changes + diff |
| `index_rebuild` | scope (tantivy\|qdrant\|both) | Stats + duration |
| `audit_diff` | since (timestamp) | All changes in window |
| `lint_run` | rules? | Issues found |

---

## 7. Permission Boundary

| Tool Tier | Claude Desktop | Claude Code | Codex CLI | Skill/Cron |
|---|---|---|---|---|
| Read | ✅ All | ✅ All | ✅ All | ✅ All |
| Propose (Semantic/Procedural) | ✅ | ✅ | ✅ | ✅ |
| Propose (Profile) | ✅ | ❌ | ❌ | ❌ |
| Commit (any) | ✅ (with confirm) | ❌ | ❌ | ✅ |
| Maintenance | ❌ | ✅ (manual) | ❌ | ✅ |

**Rationale:**
- Profile เปลี่ยนยาก → เฉพาะ Desktop (ที่คุณคุยตรงๆ) มี privilege
- Commit ใน chat = Desktop เท่านั้น เพื่อให้คุณ review diff ก่อน
- Maintenance รัน manual ผ่าน Claude Code (เป็น dev tool) หรือ cron skill
- Codex CLI = read-mostly เพราะใช้แบบ ad-hoc

Implementation: ใช้ header `X-MCP-Client-ID` + token per client, brain-mcp check ก่อน dispatch

---

## 8. Consolidation Cycle (Dream Pass)

Schedule: รายสัปดาห์ (cron skill) + manual trigger ได้

### 8.1 Stage A: Dedupe Candidates
- หา page ที่ title cosine similarity ≥0.85 หรือ content overlap ≥70%
- **ห้าม auto-merge** — output เป็น `consolidate_run` report
- คุณ review แล้วเรียก `consolidate_apply` รายการที่เห็นด้วย

### 8.2 Stage B: Contradiction Detection
- ใช้ LLM (claude-haiku ก็พอ) เปรียบเทียบ pairs ของ pages ที่ tag/topic ทับซ้อน
- ตัวอย่าง: page A "Qdrant supports SQL filter" vs page B "Qdrant uses payload filter, not SQL"
- Flag `status: contested` ทั้ง 2 หน้า + สร้าง decision draft ให้ resolve

### 8.3 Stage C: Stale Detection
- Page ที่ last_modified > 12 เดือน + ไม่มี inbound link ใน 6 เดือน → flag `stale`
- ไม่ลบ — แค่ deprioritize ใน search

### 8.4 Stage D: Procedure Health Check
- Run `verification` block ของ procedural ที่ `verified_count ≥ 1` แบบ dry-run
- ถ้า dry-run pass: bump `last_verified`
- ถ้า fail: ไม่ demote auto — แต่ flag `needs_review`

### 8.5 Stage E: Audit Diff
- สรุปทุก write ใน 7 วัน + Dream pass output ส่งเป็นรายงานให้คุณ review

---

## 9. Deployment Topology (Oracle Cloud Always Free)

### 9.1 Host Spec
- **Shape:** VM.Standard.A1.Flex (Ampere ARM)
- **Resources:** 4 OCPU + 24 GB RAM + 200 GB block storage
- **OS:** Ubuntu 24.04 LTS ARM64

### 9.2 Service Layout (single host, no containers จำเป็น)

| Service | Resource | Port | Auto-restart |
|---|---|---|---|
| llm-wiki engine | ~300 MB RAM | 18765 (HTTP), stdio (ACP) | systemd |
| Qdrant | ~2 GB @ 10K vectors | 6333 (gRPC), 6334 (HTTP) | systemd |
| Embedding service (FastEmbed/Infinity) | ~3 GB RAM | 8001 | systemd |
| Reranker service | ~2 GB RAM | 8002 | systemd |
| brain-mcp gateway | ~200 MB RAM | 8765 | systemd |
| OS + buffer | ~5 GB | | |
| **Total** | **~12 GB used / 24 GB available** | | |

Headroom 12GB เผื่อ peak load + index rebuild

### 9.3 Networking
- **Public exposure:** ❌ ไม่เปิด port ตรง
- **Access:** Tailscale หรือ Cloudflare Tunnel เท่านั้น
- **Why:** MCP server เก็บข้อมูลส่วนตัวล้วน — ห้ามให้ scan เห็น
- **MCP transport:** HTTP/SSE ผ่าน Tailscale Magic DNS (`brain.tailnet.ts.net:8765`)

### 9.4 Backup
- **Primary:** daily git push ไป private GitHub repo (00:00 UTC)
- **Qdrant snapshot:** weekly → upload เป็น tar ไป Oracle Object Storage (Free Tier มี 20GB)
- **DR:** rebuild Qdrant index จาก markdown ได้ทั้งหมด — git repo คือ source of truth

### 9.5 Why ไม่ใช้ Docker
- เพิ่ม layer ไม่จำเป็น
- systemd จัดการเรียบร้อย
- ARM image บางตัวยัง flaky
- Single host ไม่ต้อง orchestration

ถ้าอยากใช้ Docker — Compose file ทำได้ แต่ไม่ recommended สำหรับ Free Tier

---

## 10. Phased Rollout

### Phase 0 — Foundation (Week 1)
**Goal:** llm-wiki รันได้บน Oracle VM, Claude Desktop เชื่อมต่อได้

- [ ] Provision Oracle VM ARM Ampere Always Free
- [ ] Install Rust toolchain
- [ ] Clone + build llm-wiki จาก source
- [ ] สร้าง wiki space `~/wikis/brain`
- [ ] Setup Tailscale บน VM + local
- [ ] Configure Claude Desktop MCP config
- [ ] **DoD:** เปิด Claude Desktop, `wiki_list` ทำงาน, ได้ empty list

### Phase 1 — Profile Read-Only (Week 1-2)
**Goal:** Profile โหลดทุก session

- [ ] เขียน `profile/identity.md`, `hard-rules.md`, `style-guide.md` ด้วยมือ
- [ ] เขียน skill `.claude/skills/load-profile.skill.md` ที่อ่าน profile ตอน session start
- [ ] Test: เปิด chat ใหม่ ถาม "rule ของฉันคืออะไร" → ได้ตรงจาก profile
- [ ] **DoD:** 3 รอบติด ได้คำตอบสอดคล้องกับ profile.md

### Phase 2 — Semantic Wiki (Week 3-4)
**Goal:** ค้น semantic ได้, ingest source ทำงาน

- [ ] Ingest 10 sources แรก (test docs, blog posts)
- [ ] ทดสอบ `wiki_search` → BM25 อย่างเดียว
- [ ] เขียน 5 concept pages
- [ ] **DoD:** ค้น Thai keyword ได้, ค้น concept ได้

### Phase 3 — Vector Layer (Week 5-6)
**Goal:** Qdrant + BGE-M3 + hybrid retrieval

- [ ] Install Qdrant on VM
- [ ] Deploy embedding service (FastEmbed BGE-M3 หรือ Infinity)
- [ ] เขียน ingestion pipeline: markdown → chunk → embed → Qdrant
- [ ] Reembed semantic ที่มีอยู่
- [ ] เขียน `semantic_search` (hybrid) ใน brain-mcp
- [ ] **DoD:** ค้น Thai query แบบ semantic ได้ ผลดีกว่า BM25 อย่างเดียว (manual eval 10 queries)

### Phase 4 — Reranker (Week 7)
**Goal:** Top-10 ผลแม่นขึ้น

- [ ] Deploy BGE-reranker-v2-m3
- [ ] เพิ่ม rerank stage ใน pipeline
- [ ] **DoD:** precision@5 ของ eval set ดีขึ้น ≥15%

### Phase 5 — Procedural Store (Week 8-9)
**Goal:** Runbook + promotion workflow

- [ ] เขียน schema + validator สำหรับ procedural type
- [ ] เขียน `procedural_find`, `procedural_get`, `procedure_promote`
- [ ] เขียน runbook แรก (deploy brain-mcp ตัวเอง — meta!)
- [ ] **DoD:** Claude Code ค้น runbook + execute ตาม steps ได้

### Phase 6 — Propose/Commit Gate (Week 10)
**Goal:** No silent writes

- [ ] เขียน draft store + confirm token logic
- [ ] เพิ่ม `_propose` + `_commit` ทุก write
- [ ] **DoD:** ลองให้ chat เขียน profile → ต้องเห็น diff + รอ confirm

### Phase 7 — Consolidation Skill (Week 11-12)
**Goal:** Dream pass ทำงาน + audit

- [ ] เขียน consolidate skill (Markdown + Python script)
- [ ] รัน dry-run ครั้งแรก, review output
- [ ] Schedule weekly cron
- [ ] **DoD:** consolidate dry-run พบ dedup candidates + ไม่มี data loss ใน apply

### Phase 8 — Graph + Permission (Week 13+)
**Goal:** Polish, hardening

- [ ] Per-client permission via header token
- [ ] graph_neighbors tool (เริ่มจาก petgraph)
- [ ] Monitoring dashboard (Grafana + Prometheus หรือ simple log)
- [ ] **DoD:** ทุก write ใน audit log ติดตามได้, latency ตรง budget

---

## 11. Performance Budget

| Operation | Target | Stretch | Hard limit |
|---|---|---|---|
| `profile_get` | <100ms | <50ms | 200ms |
| `semantic_search` top-10 (1K pages) | <500ms | <300ms | 1000ms |
| `semantic_search` top-10 (10K pages) | <800ms | <500ms | 1500ms |
| `procedural_find` | <300ms | <150ms | 600ms |
| `wiki_ingest` 1 page | <2s | <1s | 5s |
| Full reembed (1K pages) | <20min | <10min | 30min |
| Consolidate dry-run (1K pages) | <5min | <2min | 10min |

ถ้าหลุด stretch → optimize. ถ้าหลุด target → investigate. ถ้าหลุด hard → page (alert)

---

## 12. Monitoring & Observability

### 12.1 Structured logs (JSONL)
ทุก MCP call log:
```json
{"ts":"2026-05-23T10:00:00Z","client":"claude-desktop","tool":"semantic_search",
 "args_hash":"sha256:...","duration_ms":423,"result_count":10,"status":"ok"}
```

### 12.2 Metrics (Prometheus format, scrape โดย Grafana ใน VM อื่นหรือ local)
- `brain_mcp_tool_calls_total{tool, status}`
- `brain_mcp_tool_duration_seconds{tool}` (histogram)
- `brain_qdrant_query_duration_seconds`
- `brain_embedding_duration_seconds`
- `brain_pages_total{type, status}`
- `brain_index_freshness_seconds` (last index update)

### 12.3 Audit log (git + JSONL)
- ทุก commit ใน git = audit
- เพิ่ม JSONL log สำหรับ search queries (ไม่มี write impact แต่อยากรู้)

### 12.4 Alerts (minimal)
- Latency p95 > hard limit 5 นาทีติด
- Disk > 80%
- Qdrant down
- Backup git push fail

---

## 13. Definition of Done (System-Level)

ระบบ "เสร็จ" เมื่อ:

1. ✅ **Implemented** — ทุก tool ใน §6 ทำงานครบ
2. ✅ **Tested** — integration test fixture:
   - write profile in Desktop → recall in Code → bytewise equal
   - ingest source → semantic_search → page appears in top-3
   - propose Profile → commit → git log shows correct attribution
3. ✅ **Verified with evidence** — eval set 50 queries, precision@5 ≥ 0.7
4. ✅ **Recoverable** —
   - `git revert` ใช้ได้ทุก commit
   - rebuild Qdrant จาก markdown ทำได้ <30min
   - DR drill: restore from backup สำเร็จ
5. ✅ **Explainable** —
   - README + architecture diagram
   - ADR 5+ ฉบับ
   - ใครก็ตามที่อ่าน blueprint นี้ + repo แล้ว setup ตามได้ใน 1 วัน

---

## 14. Architectural Decision Records (ADRs)

### ADR-001: Choose Qdrant over pgvector/Milvus/Weaviate
**Context:** ต้อง vector DB รองรับ payload filter + ARM + scale 10K+
**Decision:** Qdrant
**Rationale:** Rust ecosystem alignment, payload filter rวดเร็ว, HNSW + scalar quantization built-in, ARM official builds, embedded mode มี
**Consequences:** เพิ่ม service ที่ต้อง maintain (vs pgvector ที่อยู่ใน Postgres เดียว), แต่ได้ performance + filter capability

### ADR-002: Choose BGE-M3 over alternatives
**Context:** ต้อง embedding model รองรับไทย, self-host ได้, ขนาด manageable
**Decision:** BGE-M3
**Rationale:** Native Thai support, 8K context (3x ของรุ่นทั่วไป), multi-vector ในโมเดลเดียว ลด complexity, Apache 2.0
**Consequences:** RAM 3GB ระหว่างใช้งาน, ARM CPU inference ช้ากว่า GPU (~50ms/chunk) — acceptable

### ADR-003: Extend llm-wiki, don't fork or rewrite
**Context:** llm-wiki เก่ง wiki + BM25 + git แต่ไม่มี vector
**Decision:** เขียน companion `brain-mcp` ที่ wrap llm-wiki + เพิ่ม layer ของตัวเอง ไม่ fork engine
**Rationale:** llm-wiki maintainer active, ของเขา stable, fork = maintenance burden. Extension pattern: wrap MCP calls + add own tools
**Consequences:** brain-mcp กลายเป็น gateway, latency เพิ่ม ~10ms ต่อ call (acceptable)

### ADR-004: Defer graph DB until 5K pages
**Context:** Graph queries อาจจำเป็นในอนาคต
**Decision:** v1 ใช้ petgraph (in-memory) ของ llm-wiki, v2 ค่อยพิจารณา KuzuDB
**Rationale:** Premature optimization, petgraph เร็วและพอจน 5K nodes
**Consequences:** ถ้า need graph query ก่อนถึง 5K → revisit early

### ADR-005: Reject Working/Episodic stores in v1
**Context:** เดิมออกแบบ 5 layers; user ตัดสินใจเอา 3
**Decision:** เริ่มจาก 3 stores (Profile/Semantic/Procedural) เท่านั้น
**Rationale:** Working memory = harness concern. Episodic = อยู่ใน git log อยู่แล้วถ้าจำเป็นจริง. ลด surface area → ลด failure mode
**Consequences:** ไม่มี time-based recall ("จำได้ไหมตอนเราคุยเรื่อง X เมื่ออาทิตย์ก่อน") — ถ้าต้องการจริงค่อยเพิ่ม episodic เป็น append-only JSONL ภายหลัง

### ADR-006: No silent writes
**Context:** Memory pollution คือ failure mode อันดับ 1 ของ memory system
**Decision:** ทุก write ต้องผ่าน propose → confirm flow
**Rationale:** Implementation ห้าม verify ตัวเอง — propose แสดง diff + รอ explicit confirm
**Consequences:** UX มีขั้นตอนเพิ่ม ยอมรับเพื่อ safety

### ADR-007: Profile not in Qdrant
**Context:** Profile เล็กแต่สำคัญ ต้องโหลดเร็ว
**Decision:** Profile = markdown ตรงๆ, อ่านเข้า RAM cache, ไม่ใส่ Qdrant
**Rationale:** <3KB total, scan ตรง <5ms vs Qdrant ~50ms + risk ของ stale index
**Consequences:** Profile ค้นไม่ได้แบบ semantic — acceptable เพราะมี section ชัด

---

## 15. Alternative Repos Considered

ที่พิจารณาเป็น "base MCP" แต่เลือก llm-wiki:

### `getzep/graphiti`
- **Strengths:** Temporal knowledge graph (น่าสนใจสำหรับ "เมื่อก่อน vs ตอนนี้"), MCP support, Neo4j-backed
- **Weaknesses:** Neo4j dependency หนัก, schema graph-first ไม่ใช่ markdown-first, lock-in มากกว่า
- **Verdict:** Watch list — ถ้า v2 ต้องการ temporal reasoning มาดูใหม่
- **Repo:** `github.com/getzep/graphiti`

### `mem0ai/mem0`
- **Strengths:** Memory-focused, multi-layer (user/session/agent), MCP, Qdrant integration
- **Weaknesses:** Memory ≠ Knowledge wiki — schema ไม่เหมาะกับ concept compounding, designed for agent use case, ไม่ git-backed
- **Verdict:** ดีในเชิง memory primitive, ใช้แนวคิดได้แต่ไม่ใช่ base
- **Repo:** `github.com/mem0ai/mem0`

### `basic-machines/basic-memory`
- **Strengths:** Markdown + SQLite + MCP, ง่าย, lightweight
- **Weaknesses:** ไม่มี vector search, ไม่มี graph, ไม่มี typed schema validation
- **Verdict:** ดีถ้า scope เล็กกว่านี้ — ของเราต้องการ heavier infrastructure
- **Repo:** `github.com/basicmachines-co/basic-memory`

### `letta-ai/letta` (เดิม MemGPT)
- **Strengths:** Memory architecture mature, OS-style memory hierarchy
- **Weaknesses:** Heavier framework, lock-in กับ Letta agent runtime, ไม่ใช่ pure MCP server
- **Verdict:** ไม่เหมาะ — เราอยากแค่ MCP, ไม่อยากเอา agent framework
- **Repo:** `github.com/letta-ai/letta`

### `cole-medin/mcp-mem0` หรือ `chroma-core/chroma-mcp`
- **Strengths:** Lightweight MCP wrapper
- **Weaknesses:** Single-purpose, ไม่มี wiki/knowledge layer
- **Verdict:** ใช้เป็น component ได้แต่ไม่ใช่ base

### **Decision:** `geronimo-iia/llm-wiki` เป็น base + `brain-mcp` ของเราเอง wrap ข้างบน + Qdrant + BGE-M3 + Reranker

---

## 16. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| llm-wiki maintainer ยกเลิก project | Low | Medium | Fork ได้, schema คือ markdown — portable |
| BGE-M3 model deprecated | Low | Low | `embedding_version` field รองรับ migration |
| Qdrant data corruption | Low | High | Daily backup + rebuild from markdown ได้ |
| Oracle Free Tier ยกเลิก | Medium | High | Wiki = git repo, ย้ายไป VM อื่นได้ใน <1 ชม |
| Prompt injection ผ่าน source ingest | Medium | Medium | Source content ถูก quote เสมอ, ไม่ execute |
| Memory pollution จาก mistake writes | Medium | High | Propose/commit gate + git revert |
| BGE-M3 ARM CPU ช้าเกินไป | Low | Medium | Fallback: API embedding (Cohere/OpenAI) |
| Concurrent write conflict | Low | Low | Single-writer lock file + git stash |

---

## 17. Open Questions (Resolve Before Phase 3)

1. **Embedding service: FastEmbed vs Infinity vs llama.cpp?**
   - FastEmbed: ONNX, ง่าย, ไทย OK
   - Infinity: rust-based, fast, native BGE support
   - llama.cpp: ARM optimized, แต่ embedding model support varies
   - → **Decide ใน Phase 3 kickoff**

2. **Reranker: in-process หรือ separate service?**
   - In-process: latency ต่ำกว่า, แต่ memory shared กับ embedding
   - Separate: scale independent, แต่ network hop
   - → **Default: separate, revisit ถ้า latency หลุด budget**

3. **Drafts: filesystem vs in-memory?**
   - Filesystem: survive restart, audit ฟรี
   - In-memory: ลด I/O
   - → **Filesystem (boring tech, audit ดี)**

4. **Multi-wiki spaces?**
   - llm-wiki รองรับ `wiki_spaces_*` หลาย space
   - ต้องการแยก work/personal ไหม?
   - → **เริ่มจาก single space `brain`, แยกเมื่อ pain ชัด**

5. **API key management for fallback embedding?**
   - ถ้า BGE-M3 self-host พัง, fallback ไป OpenAI/Cohere
   - เก็บ key ที่ไหน? (1Password CLI? sops? plain env?)
   - → **sops + age key ใน Phase 7**

---

## 18. Future Roadmap (v2+)

หลัง v1 stable แล้ว ของที่อยู่ใน watch list:

- **Episodic store** (append-only JSONL, time-indexed retrieval) — ถ้าพบว่าต้องการ "เมื่อก่อนเราตัดสินใจอะไร" บ่อย
- **Multi-modal:** ingest รูป diagram → caption + embed via vision model
- **Temporal graph:** edge มี timestamp + validity window (กรณีอ้างอิง graphiti)
- **Federated wiki:** sync ระหว่างหลาย wiki (เช่น public/private)
- **Public wiki publishing:** llm-wiki-hugo-cms render เป็น static site
- **Real-time ingest:** watch folder → auto-ingest (vs ตอนนี้ manual)
- **Voice query:** Speech-to-text → semantic_search → TTS response (สำหรับ mobile)

---

## 19. Quick Reference Card

```
Stores:
  profile/      → identity, rules, style (NO decay, supersedes)
  semantic/     → concepts, entities, sources, decisions (compound)
  procedural/   → runbooks with verification (promotion-gated)

Backends:
  Markdown + Git    → canonical source of truth
  Tantivy (BM25)    → keyword index (in llm-wiki)
  Qdrant            → dense + sparse vectors
  Petgraph          → in-memory typed graph (→ KuzuDB at 5K)

Embedding:
  Model: BGE-M3 (1024 dim dense + sparse)
  Chunk: 512 tokens, 64 overlap, section-aware
  Rerank: BGE-reranker-v2-m3 (cross-encoder)

Pipeline:
  Query → [BM25 || Dense || Sparse] → RRF fuse → Rerank → Top-10

Write flow:
  propose → diff preview → confirm token → commit → git

Access:
  Tailscale only (no public ports)
  Daily git push backup
  Weekly Qdrant snapshot to Object Storage
```

---

**Document version:** 1.0
**Last updated:** 2026-05-23
**Next review:** หลัง Phase 2 complete (Week 4)
**Owner:** Self
**Implementer:** Self + Claude Code

---

*End of Blueprint v1.0*