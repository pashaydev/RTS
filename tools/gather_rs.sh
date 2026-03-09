#!/bin/bash

# Set the directory to search (defaults to current directory if not provided)
SEARCH_DIR="${1:-.}"

# Set the output file name (defaults to combined_code.txt if not provided)
OUTPUT_FILE="${2:-combined_code.txt}"

# Check if the search directory exists
if [ ! -d "$SEARCH_DIR" ]; then
    echo "Error: Directory '$SEARCH_DIR' does not exist."
    exit 1
fi

# Clear the output file if it already exists, or create a new one
> "$OUTPUT_FILE"

echo "Searching for .rs files in '$SEARCH_DIR'..."
echo "Outputting to '$OUTPUT_FILE'..."

# Find all .rs files recursively.
# Using -print0 and read -d '' safely handles any file names with spaces.
find "$SEARCH_DIR" -type f -name "*.rs" -print0 | while IFS= read -r -d '' file; do

    # 1. Write the path to the file in brackets
    echo "[$file]" >> "$OUTPUT_FILE"

    # 2. Write the content of the file
    cat "$file" >> "$OUTPUT_FILE"

    # 3. Add a couple of empty lines for readability between files
    echo -e "\n\n" >> "$OUTPUT_FILE"

done

echo "Done! All Rust files have been combined into '$OUTPUT_FILE'."
