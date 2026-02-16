SELECT
    f.*,
    v.*
FROM files f
JOIN versions v ON f.content_hash = v.content_hash
ORDER BY f.discovered_at DESC
LIMIT ?
