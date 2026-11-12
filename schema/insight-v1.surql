-- The insight store's schema, v1 (brain/ruststeve/design/insight-store.md): the whole contract
-- between ruststeve and steve. steve opens the store read-only and follows this file; changes
-- are accretion only (add tables, fields and views; never rename or repurpose one), posted on
-- the issue that makes them.
--
-- Built by `ingest` (crates/ingest, surrealdb = 4.0.1, SurrealKV, versioned) from race batch
-- and case files. Published as the release asset `insight.surrealkv.tar.zst` on the fixed tag
-- `insight-latest`. Open it with @surrealdb/node 3.2.0 (@surrealdb/node-native 4.x) at
-- `surrealkv://<unpacked dir>?versioned=true`, namespace `ruststeve`, database `insight`.
--
-- Ids are stable: a run is run:[<race run id>, <bot name>], the same pair the live snapshot
-- stream carries, so a live bot links to its row.

-- The schema's own version, one row: meta:schema.
DEFINE TABLE IF NOT EXISTS meta SCHEMALESS;

-- A commit that raced.
DEFINE TABLE IF NOT EXISTS commit SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS sha ON commit TYPE string;
DEFINE FIELD IF NOT EXISTS short ON commit TYPE string;

-- The stage ladder, lowest rank first (world::Stage).
DEFINE TABLE IF NOT EXISTS stage SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS name ON stage TYPE string;
DEFINE FIELD IF NOT EXISTS rank ON stage TYPE int;

-- A checkpoint start (config/starts.toml), by name.
DEFINE TABLE IF NOT EXISTS start SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS name ON start TYPE string;

-- One lane of one race: batch:[<race run id>, <lane>].
DEFINE TABLE IF NOT EXISTS batch SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS race_run ON batch TYPE string;
DEFINE FIELD IF NOT EXISTS commit ON batch TYPE string;
DEFINE FIELD IF NOT EXISTS lane ON batch TYPE int;
DEFINE FIELD IF NOT EXISTS start ON batch TYPE string;
DEFINE FIELD IF NOT EXISTS source ON batch TYPE string;              -- "real" | "sim"
DEFINE FIELD IF NOT EXISTS config_hash ON batch TYPE string;
DEFINE FIELD IF NOT EXISTS bots ON batch TYPE int;
DEFINE FIELD IF NOT EXISTS server ON batch TYPE string;
DEFINE FIELD IF NOT EXISTS server_tick_rate ON batch TYPE float;
DEFINE FIELD IF NOT EXISTS runs ON batch TYPE int;
DEFINE FIELD IF NOT EXISTS cases ON batch TYPE int;
DEFINE FIELD IF NOT EXISTS recordings ON batch TYPE int;
DEFINE FIELD IF NOT EXISTS coverage ON batch TYPE float;             -- cases / runs
DEFINE FIELD IF NOT EXISTS events_dropped ON batch TYPE int;
DEFINE FIELD IF NOT EXISTS unknown_packets ON batch TYPE int;
DEFINE FIELD IF NOT EXISTS artifact ON batch TYPE string;            -- the lane's workflow artifact name

-- One bot's run, with its whole case: run:[<race run id>, <bot>].
DEFINE TABLE IF NOT EXISTS run SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS race_run ON run TYPE string;
DEFINE FIELD IF NOT EXISTS bot ON run TYPE string;
DEFINE FIELD IF NOT EXISTS commit ON run TYPE string;
DEFINE FIELD IF NOT EXISTS lane ON run TYPE int;
DEFINE FIELD IF NOT EXISTS start ON run TYPE string;
DEFINE FIELD IF NOT EXISTS source ON run TYPE string;
DEFINE FIELD IF NOT EXISTS stage_reached ON run TYPE option<string>;  -- a stage name, none if none
DEFINE FIELD IF NOT EXISTS stage_rank ON run TYPE int;               -- -1 when none
DEFINE FIELD IF NOT EXISTS ending ON run TYPE string;                -- death|cap|kicked|lost|error|cut
DEFINE FIELD IF NOT EXISTS counted ON run TYPE bool;                 -- false for cut
DEFINE FIELD IF NOT EXISTS killer ON run TYPE string;                -- signature's second part; "" if none
DEFINE FIELD IF NOT EXISTS doing ON run TYPE string;
DEFINE FIELD IF NOT EXISTS why ON run TYPE string;
DEFINE FIELD IF NOT EXISTS server_ticks ON run TYPE int;
DEFINE FIELD IF NOT EXISTS wall_ms ON run TYPE int;
DEFINE FIELD IF NOT EXISTS recording ON run TYPE option<string>;
DEFINE FIELD IF NOT EXISTS replay ON run TYPE string;
-- The case file as written (results::Case), every field by name.
DEFINE FIELD IF NOT EXISTS case ON run TYPE object FLEXIBLE;

-- Graph relations.
DEFINE TABLE IF NOT EXISTS ran TYPE RELATION FROM commit TO run;
DEFINE TABLE IF NOT EXISTS reached TYPE RELATION FROM run TO stage;
DEFINE TABLE IF NOT EXISTS played TYPE RELATION FROM run TO start;
DEFINE TABLE IF NOT EXISTS in_batch TYPE RELATION FROM run TO batch;

-- One row per counted run and stage: did the run reach it? The k/n view folds these.
-- attempt:[<race run id>, <bot>, <stage name>].
DEFINE TABLE IF NOT EXISTS attempt SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS commit ON attempt TYPE string;
DEFINE FIELD IF NOT EXISTS start ON attempt TYPE string;
DEFINE FIELD IF NOT EXISTS source ON attempt TYPE string;
DEFINE FIELD IF NOT EXISTS stage ON attempt TYPE string;
DEFINE FIELD IF NOT EXISTS rank ON attempt TYPE int;
DEFINE FIELD IF NOT EXISTS reached ON attempt TYPE int;              -- 1 or 0

-- Views, kept current as batches arrive.
-- k/n per stage, per commit, start and source (real and sim apart).
DEFINE TABLE IF NOT EXISTS kn AS
    SELECT commit, start, source, stage, rank, math::sum(reached) AS k, count() AS n
    FROM attempt GROUP BY commit, start, source, stage, rank;
-- Deaths by killer and stage reached, per commit.
DEFINE TABLE IF NOT EXISTS deaths AS
    SELECT commit, stage_reached, killer, count() AS n
    FROM run WHERE ending = "death" GROUP BY commit, stage_reached, killer;
-- Endings by signature, per commit and start: what the batch ranks.
DEFINE TABLE IF NOT EXISTS endings AS
    SELECT commit, start, ending, killer, doing, why, count() AS n
    FROM run GROUP BY commit, start, ending, killer, doing, why;
