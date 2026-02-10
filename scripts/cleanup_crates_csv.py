"""
Quick script to clean up the crates.io official data dump at
https://static.crates.io/db-dump.tar.gz

For more info see: https://crates.io/data-access
"""

import csv
import sys
from pathlib import Path

# Deal with very huge fields in the data dump
csv.field_size_limit(sys.maxsize)

BASE = Path("/path/to/db-dump/data")
CRATES_CSV = BASE / "crates.csv"
CRATE_DOWNLOADS_CSV = BASE / "crate_downloads.csv"
OUT_CSV = Path("data/crate-lists/all.csv")
OUT_CSV_FIRST10000 = Path("data/crate-lists/top10000.csv")

# Load total downloads per crate
print("Loading crate_downloads.csv...")
downloads_map = {}
with open(CRATE_DOWNLOADS_CSV, newline='', encoding='utf-8') as infile:
    reader = csv.reader(infile, delimiter=',')
    header = next(reader)
    for row in reader:
        crate_id = row[0]
        downloads = int(row[1])
        downloads_map[crate_id] = downloads

print(f"Loaded {len(downloads_map):,} download entries")

# Parse crates.csv
print("Loading crates.csv...")
rows = []
with open(CRATES_CSV, newline='', encoding='utf-8') as infile:
    reader = csv.reader(infile, delimiter=',')
    header = next(reader)

    for row in reader:
        # 0. created_at
        # 1. description
        # 2. documentation
        # 3. homepage
        # 4. id
        # 5. max_features
        # 6. max_upload_size
        # 7. name
        # 8. readme
        # 9. repository
        # 10. updated_at
        created_at = row[0]
        description = row[1]
        documentation = row[2]
        homepage = row[3]
        crate_id = row[4]
        name = row[7]
        repository = row[9]
        updated_at = row[10]

        # Cleanup:
        # description sometimes contains newlines. And is sometimes too long
        if '\n' in description:
            description = description.replace('\n', ' ')
        if len(description) > 100:
            description = description[:97] + "..."

        downloads = downloads_map.get(crate_id, 0)

        out_row = [
            name,
            downloads,
            description,
            created_at,
            updated_at,
            documentation,
            homepage,
            repository,
            crate_id,
        ]
        rows.append(out_row)

print(f"Parsed {len(rows):,} crates")

# Sort by downloads (descending)
rows.sort(reverse=True, key=lambda x: x[1])

# Write all crates
OUT_CSV.parent.mkdir(parents=True, exist_ok=True)
with open(OUT_CSV, 'w', newline='', encoding='utf-8') as outfile:
    writer = csv.writer(outfile, delimiter=',', lineterminator='\n')
    writer.writerow([
        "name",
        "downloads",
        "description",
        "created_at",
        "updated_at",
        "documentation",
        "homepage",
        "repository",
        "id",
    ])
    writer.writerows(rows)
    print(f"Wrote {len(rows):,} rows to {OUT_CSV}")

# Write top 10000 by downloads
top10000 = rows[:10000]
with open(OUT_CSV_FIRST10000, 'w', newline='', encoding='utf-8') as outfile:
    writer = csv.writer(outfile, delimiter=',', lineterminator='\n')
    writer.writerow([
        "name",
        "downloads",
        "description",
        "created_at",
        "updated_at",
        "documentation",
        "homepage",
        "repository",
        "id",
    ])
    writer.writerows(top10000)
    print(f"Wrote {len(top10000):,} rows to {OUT_CSV_FIRST10000}")
