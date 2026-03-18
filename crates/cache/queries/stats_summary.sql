SELECT
    COUNT(DISTINCT v.work_id) AS works,
    COUNT(DISTINCT v.content_hash) AS versions,
    COUNT(*) AS files,
    COALESCE(SUM(v.words), 0) AS total_words,
    COALESCE(SUM(v.content_size), 0) AS total_content_size
FROM files f
INNER JOIN versions v ON v.content_hash = f.content_hash
WHERE f.target = ?
