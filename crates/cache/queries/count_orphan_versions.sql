SELECT COUNT(*)
FROM versions v
WHERE v.content_hash NOT IN (
    SELECT f.content_hash
    FROM files f
)
