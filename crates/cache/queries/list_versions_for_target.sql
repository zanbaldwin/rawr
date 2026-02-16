SELECT
    f.*,
    v.*
FROM versions v
JOIN files f ON f.content_hash = v.content_hash
WHERE f.target = ?
ORDER BY v.work_id DESC, v.extracted_at DESC
