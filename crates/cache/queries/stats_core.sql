WITH base AS (
    SELECT
        f.file_size,
        v.work_id,
        v.content_hash,
        v.words,
        v.content_size,
        v.complete,
        v.published_on
    FROM files f
    INNER JOIN versions v ON v.content_hash = f.content_hash
    WHERE f.target = ?1
),
agg AS (
    SELECT
        COUNT(DISTINCT work_id) AS works,
        COUNT(DISTINCT content_hash) AS versions,
        COUNT(*) AS files,
        COALESCE(SUM(words), 0) AS total_words,
        COALESCE(SUM(content_size), 0) AS total_content_size,
        COALESCE(SUM(file_size), 0) AS total_file_size,
        MIN(published_on) AS oldest_published,
        MAX(published_on) AS newest_published
    FROM base
),
work_agg AS (
    SELECT
        COALESCE(SUM(best_words), 0) AS unique_words,
        COALESCE(SUM(CASE WHEN is_complete = 1 THEN 1 ELSE 0 END), 0) AS complete_works,
        COALESCE(SUM(CASE WHEN is_complete = 0 THEN 1 ELSE 0 END), 0) AS incomplete_works
    FROM (
        SELECT MAX(words) AS best_words, MAX(complete) AS is_complete
        FROM base
        GROUP BY work_id
    )
)
SELECT
    agg.works,
    agg.versions,
    agg.files,
    agg.total_words,
    agg.total_content_size,
    agg.total_file_size,
    agg.oldest_published,
    agg.newest_published,
    work_agg.unique_words,
    work_agg.complete_works,
    work_agg.incomplete_works
FROM agg, work_agg
