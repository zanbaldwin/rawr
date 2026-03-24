WITH recent_works AS (
    SELECT v.work_id
    FROM files f
    JOIN versions v ON f.content_hash = v.content_hash
    WHERE f.target = ?
    GROUP BY v.work_id
    ORDER BY MAX(f.discovered_at) DESC
    LIMIT ?
)
SELECT
    f.*,
    v.*
FROM files f
LEFT JOIN versions v ON f.content_hash = v.content_hash
WHERE v.work_id IN (SELECT work_id FROM recent_works)
  AND f.target = ?
