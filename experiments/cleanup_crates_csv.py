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

        # Cleanup:
        # Index 8 is the offending row (a README dump) causing most
        # of the bloat
        # Index 6 doesn't seem to contain any data
        del row[8]
        del row[6]

        rows.append(row)

    # Sort data by downloads
    first_row = rows[0]
    rows = rows[1:]
    rows.sort(reverse=True, key=lambda x: x[3])
    rows = [first_row] + rows

    with open(OUT_CSV, 'w', newline='') as outfile:
        out_writer = csv.writer(outfile, delimiter=',', lineterminator='\n')
        for row in rows:
            out_writer.writerow(row)
