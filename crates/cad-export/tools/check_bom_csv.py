"""Independent standard-library CSV interoperability check against BOM JSON.

Usage: python3 check_bom_csv.py output.bom.csv output.bom.json
No runtime CAD dependency. Textual cells are compared verbatim; missing numeric
values must be empty. Numeric values and quantity must match the JSON report.
"""
import argparse
import csv
import json
import math

COLUMNS = ["part_id", "part_number", "revision", "description", "material",
           "density_kg_per_m3", "quantity", "unit_volume_mm3", "total_volume_mm3",
           "unit_mass_kg", "total_mass_kg"]


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("csv")
    parser.add_argument("json")
    args = parser.parse_args()
    with open(args.json, encoding="utf-8") as source:
        report = json.load(source)
    with open(args.csv, encoding="utf-8", newline="") as source:
        reader = csv.DictReader(source, strict=True)
        require(reader.fieldnames == COLUMNS, "incorrect CSV columns")
        rows = list(reader)
    items = sorted(report["items"], key=lambda item: item["part_id"])
    require(len(rows) == len(items), "row count differs")
    for row, item in zip(rows, items):
        require(set(row) == set(COLUMNS), "row has incorrect column count")
        metadata = item["metadata"]
        material = metadata.get("material")
        wanted = {"part_id": item["part_id"],
                  **{key: metadata.get(key) or "" for key in ("part_number", "revision", "description")},
                  "material": material["name"] if material else ""}
        for key, value in wanted.items():
            require(row[key] == value, f"text differs for {item['part_id']}/{key}")
        require(row["quantity"] == str(item["quantity"]), "quantity differs")
        for key in ("density_kg_per_m3", "unit_volume_mm3", "total_volume_mm3", "unit_mass_kg", "total_mass_kg"):
            value = (material["density_kg_per_m3"] if material else None) if key == "density_kg_per_m3" else item.get(key)
            if value is None:
                require(row[key] == "", f"unknown {key} must be empty")
            else:
                number = float(row[key])
                require(math.isfinite(number) and number == value, f"numeric {key} differs")
    print(json.dumps({"valid": True, "rows": len(rows), "columns": COLUMNS}))


if __name__ == "__main__":
    main()
