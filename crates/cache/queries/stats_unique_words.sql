SELECT
    COALESCE(SUM(best_words), 0) AS words
FROM (
    SELECT MAX(v.words) AS best_words
    FROM files f
    INNER JOIN versions v ON v.content_hash = f.content_hash
    WHERE f.target = ?
    GROUP BY v.work_id
)
