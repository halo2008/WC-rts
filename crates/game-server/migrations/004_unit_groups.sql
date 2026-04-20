-- ─── Unit groups ────────────────────────────────────────────────────
-- HoI-style army groups: a named collection of units that move together as
-- a single entity. A unit belongs to at most one group at a time
-- (`units.group_id` nullable). When a group moves, the whole group gets a
-- single transit and every member's hex is kept in sync.

CREATE TABLE unit_groups (
    id                      BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    nation_id               UUID NOT NULL REFERENCES nations(id) ON DELETE CASCADE,
    name                    VARCHAR(80) NOT NULL,
    hex_q                   INT NOT NULL,
    hex_r                   INT NOT NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_unit_groups_nation ON unit_groups (nation_id);
CREATE INDEX idx_unit_groups_hex ON unit_groups (hex_q, hex_r);

ALTER TABLE units
    ADD COLUMN group_id BIGINT REFERENCES unit_groups(id) ON DELETE SET NULL;

CREATE INDEX idx_units_group ON units (group_id) WHERE group_id IS NOT NULL;

-- Transits for groups. Mirrors `unit_transits` but keyed on group_id.
-- When a group transit completes, each member unit is updated individually.
CREATE TABLE group_transits (
    group_id                BIGINT PRIMARY KEY REFERENCES unit_groups(id) ON DELETE CASCADE,
    path                    JSONB NOT NULL,
    progress_hex            REAL NOT NULL DEFAULT 0.0,
    base_speed              REAL NOT NULL,
    status                  VARCHAR(16) NOT NULL DEFAULT 'Active'
        CHECK (status IN ('Active', 'Completed', 'Cancelled', 'Blocked')),
    started_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_group_transits_status ON group_transits (status);
