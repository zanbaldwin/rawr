SELECT
    f.*,
    v.*
FROM versions v
LEFT JOIN files f ON f.content_hash = v.content_hash
WHERE v.content_hash = ?
