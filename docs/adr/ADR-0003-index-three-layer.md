# ADR-0003: 索引三层架构（SQLite / Tantivy / sqlite-vec）

| 状态 | Accepted |
| 日期 | 2026-06-21 |
| 关联 | PRD §6.1, §15.5（O5）; ADR-0001, ADR-0002 |
| 决策者 | 用户（确认 + 关键反馈：FTS5 中文不行） |

## 背景（Context）

LMNotes 的派生索引需同时支持三类查询（PRD §5.4, §5.5）：
1. **元数据/关系查询**：concept 列表、邻接表（图谱）、LLM 建议队列、快照索引。
2. **全文检索**：中英混排的 BM25 关键词搜索（FR-SEARCH-02）。
3. **向量检索**：语义搜索 + RAG（FR-LLM-02, FR-LLM-04）。

约束：
- **本地优先 + 单文件可移植**：派生索引应像 OKF bundle 一样可拷贝、可备份、可整体删除重建（ADR-0001 合规要求 1）。
- **中文检索质量**：用户明确指出 SQLite **FTS5 对中文支持不佳**（`unicode61`/`simple` 按字切、`jieba` tokenizer 需额外编译且 FTS 集成脆弱）。这是本决策的关键触发点。
- **性能**：§6.1 要求全文 P95 < 100ms、增量更新 < 1s。
- **Rust 核心契合**（ADR-0002）：优先纯 Rust 或 Rust 友好生态。
- **许可证**（O6a）：MIT/Apache 兼容。

## 决策（Decision）

**采用三层派生索引，各司其职，统一存 `.lmnotes/`，可从 OKF bundle 全量重建。**

| 层 | 选型 | 角色 | 存储 |
|---|---|---|---|
| 元数据/关系 | **SQLite** | concept 元信息、邻接表（反向链接派生表）、LLM 建议队列、快照元数据、合并状态 | `.lmnotes/index.sqlite` |
| 全文检索 | **Tantivy v0.26**（Rust 版 Lucene） | 中英混排 BM25 全文索引；中文用 **jieba-rs** 作 tokenizer | `.lmnotes/tantivy/` |
| 向量检索 | **sqlite-vec**（SQLite 扩展） | 语义检索、RAG 上下文召回 | 同 SQLite 库内虚拟表 |

### 关键设计

1. **单一真相源**：OKF bundle（`notes/` + `transcripts/` + `assets/`）是唯一真源；三层索引全部为派生数据，`.lmnotes/` 可随时删除并由 `index rebuild` 命令重建（满足 ADR-0001 合规要求 1）。
2. **SQLite 为枢纽**：Tantivy 与 sqlite-vec 各存数据，但**主键一致性由 SQLite 表统一管理**——SQLite 的 `concepts(id, path, type, mtime, hash)` 表是其它两层定位记录的依据；删除/移动 concept 时，索引器先更 SQLite，再级联删 Tantivy 文档与向量行。
3. **中文分词**：Tantivy 注册自定义 `JiebaTokenizer`（jieba-rs 实现 `TokenStream`），CJK 按词切、非 CJK 走 Tantivy 默认；多语言笔记按 frontmatter `language` 选择 tokenizer（§6.7）。
4. **混合检索融合**：查询时分别取 BM25 top-K（Tantivy）与向量 top-K（sqlite-vec），用 **RRF（Reciprocal Rank Fusion）** 融合排序，由核心层返回统一 `SearchHit { concept_id, score, source }`。前端不感知来源。
5. **增量索引**：文件保存事件 → 计算 hash → 若变则更新三层；目标 < 1s（§6.1）。后台 worker 串行化写避免竞争（ADR-0002 的"重计算在核心"）。
   - **Tantivy 更新语义（重要）**：Tantivy 不支持原地更新文档，更新 = **按 term（concept_id）删除旧文档 + 插入新文档**。索引器对每个 concept 维护稳定的 Tantivy unique term（用 frontmatter `id`），保存时先 `delete_by_term(id)` 再 `add(新文档)`。需定期 `index_writer.commit()` + 后台 `garbage_collect` 控制段碎片。
   - **sqlite-vec 更新**：标准 `DELETE + INSERT` 或 `UPDATE` 行，事务化。
   - **SQLite 元数据**：标准 `UPSERT`。
6. **图谱邻接（增量，非全量）**：邻接表存 SQLite（`edges(src_id, dst_id, link_text)`）。**增量策略**——编辑笔记 X 时只重算 X 的局部图：
   1. 解析 X 新正文的出边集合 `new_out(X)`；
   2. 从 SQLite 取 X 旧出边 `old_out(X)`；
   3. `DELETE FROM edges WHERE src_id = X.id`；`INSERT` 新出边；
   4. 反向边（X 作为 dst）由查询时 `SELECT WHERE dst_id = ?` 即时得出，无需物化——对齐 ADR-0001 决策 O2（不写 frontmatter backrefs）。
   - 复杂度：每次保存 O(X 的出边数)，非 O(全库)。全量重建仅在 `.lmnotes/` 损坏时触发。

### 调研依据（写入决策记录）

- **Tantivy v0.26.1**：crates.io 显示 14,432,945 累计下载、62 个版本、活跃维护；自我定位"closer to Apache Lucene... a crate that can be used to build such a search engine"；MIT 许可。契合"嵌入式库"需求（非服务器）。
- **jieba-rs v0.10.1**：50 版本，"Jieba Chinese Word Segmentation in Rust"，MIT，与 Tantivy tokenizer 接口契合。
- **sqlite-vec**：SQLite 扩展，单文件、MIT，与 SQLite 共栈避免多进程。

## 后果（Consequences）

**正面：**
- 中文检索质量由 jieba-rs 词级分词保证，彻底解决 FTS5 痛点。
- 三层各用最合适的工具，性能目标可达成（Tantivy BM25 P95 < 100ms）。
- 全部 MIT/Apache，兼容开源（O6a）。
- 全 Rust 侧（Tantivy/jieba-rs）+ SQLite（C，成熟绑定），契合 ADR-0002 核心层。

**负面：**
- 三套存储的一致性需索引器谨慎维护（删除/移动的级联）。缓解：SQLite 作枢纽，事务化级联；全量 rebuild 作兜底。
- 多一个数据目录（`.lmnotes/tantivy/`）。缓解：用户视角透明（导出 vault 时排除 `.lmnotes/`，ADR-0001 合规要求 2）。
- sqlite-vec 的向量规模上限（单库）对超大 vault（10 万+ concept）可能需分片。缓解：M3 评测，必要时按目录分库或迁 LanceDB（见替代方案）。

**缓解措施汇总：** SQLite 事务级联、全量 rebuild 兜底、M3 向量规模评测。

## 考虑过的替代方案（Alternatives）

- **SQLite FTS5（+ jieba tokenizer）**：**拒绝（用户明确反对）**。FTS5 的 `unicode61`/`simple` 对中文按字切不可用；jieba tokenizer 需 C 扩展编译、跨平台分发脆弱、与 Rust 核心不契合。
- **SQLite + LanceDB（向量专库）**：备选，向量检索更强（列式、增量快）。未选主方案因多一个进程式依赖、与 SQLite 共栈优势丧失。**保留为 M3 评测后的升级路径**（若 sqlite-vec 规模不足）。
- **DuckDB**：拒绝。分析型定位、向量扩展弱，对笔记应用的 OLTP 式读写（频繁单条更新）不合适。
- **Postgres + pgvector**：拒绝。违背本地优先（需服务端进程），仅在自建服务器场景才考虑，非 LMNotes 定位。
- **Tantivy 同时承担向量**（如带向量扩展）：拒绝。Tantivy 的 dense vector 支持仍实验性，sqlite-vec 更成熟。
