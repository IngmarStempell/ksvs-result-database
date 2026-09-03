CREATE VIEW IF NOT EXISTS lm_medals_with_dm_participation AS
SELECT
    athletes.id AS athlete_id,
    athletes.canonical_name AS athlete_name,
    clubs.id AS club_id,
    clubs.canonical_name AS club_name,
    lm_competitions.year AS year,
    lm_results.id AS lm_result_id,
    lm_results.rank AS lm_rank,
    lm_results.medal AS lm_medal,
    lm_results.result_kind AS lm_result_kind,
    lm_results.raw_discipline AS lm_discipline,
    lm_results.event_class AS lm_event_class,
    dm_results.id AS dm_result_id,
    dm_results.raw_payload AS dm_raw_payload,
    CASE
        WHEN dm_results.id IS NULL THEN 0
        ELSE 1
    END AS has_dm_participation
FROM results lm_results
JOIN competitions lm_competitions
    ON lm_competitions.id = lm_results.competition_id
LEFT JOIN athletes
    ON athletes.id = lm_results.athlete_id
LEFT JOIN clubs
    ON clubs.id = lm_results.club_id
LEFT JOIN results dm_results
    ON dm_results.athlete_id = lm_results.athlete_id
    AND dm_results.club_id = lm_results.club_id
    AND dm_results.participation_only = 1
    AND dm_results.competition_id IN (
        SELECT dm_competitions.id
        FROM competitions dm_competitions
        WHERE dm_competitions.year = lm_competitions.year
            AND dm_competitions.scope = 'DM'
    )
WHERE lm_competitions.scope = 'LM'
    AND lm_results.medal IS NOT NULL;
