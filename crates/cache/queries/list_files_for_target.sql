SELECT
    f.*,
    v.*
FROM files f
JOIN versions v ON f.content_hash = v.content_hash
WHERE f.target = ?
ORDER BY f.path
