INSERT INTO versions (
    content_hash,       content_crc32,  work_id,        content_size,
    title,              authors,        fandoms,        series,
    chapters_written,   chapters_total, words,          summary,
    rating,             warnings,       lang,           published_on,
    last_modified,      tags,           extracted_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (content_hash) DO NOTHING;
