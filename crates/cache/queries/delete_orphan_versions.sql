DELETE
FROM versions
WHERE content_hash NOT IN (
    SELECT f.content_hash
    FROM files f
)
