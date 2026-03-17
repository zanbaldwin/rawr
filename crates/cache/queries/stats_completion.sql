SELECT
    COALESCE(SUM(CASE WHEN complete = 1 THEN 1 ELSE 0 END), 0) AS complete,
    COALESCE(SUM(CASE WHEN complete = 0 THEN 1 ELSE 0 END), 0) AS incomplete
FROM (
    SELECT v.work_id, MAX(v.complete) AS complete
    FROM files f
    INNER JOIN versions v ON v.content_hash = f.content_hash
    WHERE f.target = ?
    GROUP BY v.work_id
)
