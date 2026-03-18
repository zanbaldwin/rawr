SELECT
    MIN(v.published_on) AS oldest,
    MAX(v.published_on) AS newest
FROM files f
INNER JOIN versions v ON v.content_hash = f.content_hash
WHERE f.target = ?
