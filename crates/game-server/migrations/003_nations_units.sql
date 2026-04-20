-- ─── Nation realtime state ──────────────────────────────────────────
-- Per-nation state that ticks at 5-minute cadence (stockpile, budget).
-- Nation metadata lives in `nations` (migration 001).

CREATE TABLE nation_state (
    nation_id               UUID PRIMARY KEY REFERENCES nations(id) ON DELETE CASCADE,
    fuel                    REAL NOT NULL DEFAULT 0.0,
    metals                  REAL NOT NULL DEFAULT 0.0,
    tech                    REAL NOT NULL DEFAULT 0.0,
    food                    REAL NOT NULL DEFAULT 0.0,
    budget_military         REAL NOT NULL DEFAULT 0.35,
    budget_economy          REAL NOT NULL DEFAULT 0.30,
    budget_research         REAL NOT NULL DEFAULT 0.15,
    budget_social           REAL NOT NULL DEFAULT 0.15,
    budget_intel            REAL NOT NULL DEFAULT 0.05,
    capital_q               INT NOT NULL DEFAULT 0,
    capital_r               INT NOT NULL DEFAULT 0,
    war_support             REAL NOT NULL DEFAULT 0.5,
    stability               REAL NOT NULL DEFAULT 0.7,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ─── Units ──────────────────────────────────────────────────────────
-- Individual strategic-map units. The UI groups them into stacks.

CREATE TABLE units (
    id                      BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    unit_type               VARCHAR(32) NOT NULL,
    nation_id               UUID NOT NULL REFERENCES nations(id) ON DELETE CASCADE,
    hex_q                   INT NOT NULL,
    hex_r                   INT NOT NULL,
    hp                      INT NOT NULL,
    max_hp                  INT NOT NULL,
    morale                  REAL NOT NULL DEFAULT 1.0,
    experience              REAL NOT NULL DEFAULT 0.0,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_units_nation ON units (nation_id);
CREATE INDEX idx_units_hex ON units (hex_q, hex_r);

-- ─── Unit transits ─────────────────────────────────────────────────
-- Live realtime movement orders. Ticked at 1 Hz by the strategic loop.
-- `path` is a JSON array of {"q": i, "r": i}. `progress_hex` is a float
-- index into that path (0 = at path[0], path_len-1 = completed).

CREATE TABLE unit_transits (
    unit_id                 BIGINT PRIMARY KEY REFERENCES units(id) ON DELETE CASCADE,
    path                    JSONB NOT NULL,
    progress_hex            REAL NOT NULL DEFAULT 0.0,
    base_speed              REAL NOT NULL,
    status                  VARCHAR(16) NOT NULL DEFAULT 'Active'
        CHECK (status IN ('Active', 'Completed', 'Cancelled', 'Blocked')),
    started_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_transits_status ON unit_transits (status);

-- ─── Production queue ──────────────────────────────────────────────
-- Units being trained at a barracks/factory. Feeds into `units` on completion.

CREATE TABLE production_queue (
    id                      BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    nation_id               UUID NOT NULL REFERENCES nations(id) ON DELETE CASCADE,
    unit_type               VARCHAR(32) NOT NULL,
    spawn_hex_q             INT NOT NULL,
    spawn_hex_r             INT NOT NULL,
    progress                REAL NOT NULL DEFAULT 0.0,
    total_time              REAL NOT NULL,
    status                  VARCHAR(16) NOT NULL DEFAULT 'InProgress'
        CHECK (status IN ('InProgress', 'Completed', 'Cancelled')),
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_production_nation ON production_queue (nation_id);
CREATE INDEX idx_production_status ON production_queue (status);

-- ─── Seed a few starting nations ───────────────────────────────────
-- Codes are ISO-style but deliberately short; names match common usage.
-- Capital hex coords are placeholders on the 1200x600 strategic grid.

INSERT INTO nations (name, code, government_type, tier, color) VALUES
    ('Stany Zjednoczone', 'USA', 'democracy',    1, '#3b82f6'),
    ('Chiny',             'CHN', 'one_party',    1, '#ef4444'),
    ('Polska',            'POL', 'democracy',    3, '#e11d48'),
    ('Brazylia',          'BRA', 'democracy',    3, '#22c55e')
ON CONFLICT (code) DO NOTHING;

-- Corresponding starting state. Tier-1 nations get larger stockpiles.
INSERT INTO nation_state (nation_id, fuel, metals, tech, food, capital_q, capital_r, war_support, stability)
SELECT n.id,
       CASE WHEN n.tier = 1 THEN 10000.0 ELSE 3000.0 END,
       CASE WHEN n.tier = 1 THEN 12000.0 ELSE 4000.0 END,
       CASE WHEN n.tier = 1 THEN  5000.0 ELSE 1500.0 END,
       CASE WHEN n.tier = 1 THEN  8000.0 ELSE 3500.0 END,
       CASE n.code
           WHEN 'USA' THEN 300
           WHEN 'CHN' THEN 900
           WHEN 'POL' THEN 650
           WHEN 'BRA' THEN 400
           ELSE 600
       END,
       CASE n.code
           WHEN 'USA' THEN 200
           WHEN 'CHN' THEN 220
           WHEN 'POL' THEN 190
           WHEN 'BRA' THEN 330
           ELSE 300
       END,
       0.5,
       0.7
FROM nations n
WHERE n.code IN ('USA', 'CHN', 'POL', 'BRA')
ON CONFLICT (nation_id) DO NOTHING;
