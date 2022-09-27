"""
Quick script to clean up the crates.io official data dump at
https://static.crates.io/db-dump.tar.gz

For more info see: https://crates.io/data-access
"""

import csv
import sys
import time

# Deal with very huge fields in the data dump
csv.field_size_limit(sys.maxsize)

CRATES_CSV = "/Users/caleb/Downloads/2022-09-25-020017/data/crates.csv"
OUT_CSV = "data/crates.csv"
OUT_CSV_FIRST1000 = "data/crates-top1000.csv"

with open(CRATES_CSV, newline='') as infile:
    in_reader = csv.reader(infile, delimiter=',')
    rows = []
    for row in in_reader:
        # 11 rows:
        # 0. created_at
        # 1. description
        # 2. documentation
        # 3. downloads
        # 4. homepage
        # 5. id
        # 6. max_upload_size
        # 7. name
        # 8. readme
        # 9. repository
        # 10. updated_at

        # Ignored rows:
        # Index 8 is the offending row (a README dump) causing most
        # of the bloat
        # Index 6 doesn't seem to contain any data
        created_at = row[0]
        description = row[1]
        documentation = row[2]
        downloads = row[3]
        homepage = row[4]
        id = row[5]
        name = row[7]
        repository = row[9]
        updated_at = row[10]

        # Cleanup:
        # description sometimes contains newlines. And is sometimes too long
        if '\n' in description:
            description = description.replace('\n', ' ')
            # print(f"Removed newlines: {description}")

        out_row = [
            created_at,
            description,
            documentation,
            downloads,
            homepage,
            id,
            name,
            repository,
            updated_at,
        ]

        rows.append(out_row)

# Sort data by downloads
first_row = rows[0]
rows = rows[1:]
rows.sort(reverse=True, key=lambda x: int(x[3]))
rows = [first_row] + rows

with open(OUT_CSV, 'w', newline='') as outfile:
    out_writer = csv.writer(outfile, delimiter=',', lineterminator='\n')
    for row in rows:
        out_writer.writerow(row)
    print(f"Successfully wrote {len(rows)} rows")

rows = rows[:1001]

with open(OUT_CSV_FIRST1000, 'w', newline='') as outfile:
    out_writer = csv.writer(outfile, delimiter=',', lineterminator='\n')
    for row in rows:
        out_writer.writerow(row)
    print(f"Successfully wrote {len(rows)} rows")
