#!/bin/bash

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROTO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

echo "Protobuf Root: $PROTO_ROOT"

cd "$SCRIPT_DIR"

find . -maxdepth 1 -name "*.proto" | while read -r PROTO_FILE_REL; do
    PROTO_FILE="$PROTO_FILE_REL" 
    echo "Processing $PROTO_FILE..."
    
    echo "  Generating Go..."
    protoc -I "$PROTO_ROOT" -I . \
        --go_out=. --go_opt=paths=source_relative \
        --go-grpc_out=. --go-grpc_opt=paths=source_relative \
        "$PROTO_FILE"
        
    echo "  Generating Rust..."
    if command -v protoc-gen-prost &> /dev/null; then
         RUST_TMP_DIR=$(mktemp -d)
         
         # Generate into temp dir with both plugins
         if command -v protoc-gen-tonic &> /dev/null; then
             protoc -I "$PROTO_ROOT" -I . \
                --prost_out="$RUST_TMP_DIR" --prost_opt=compile_well_known_types \
                --tonic_out="$RUST_TMP_DIR" --tonic_opt=compile_well_known_types \
                "$PROTO_FILE"
         else
             protoc -I "$PROTO_ROOT" -I . \
                --prost_out="$RUST_TMP_DIR" --prost_opt=compile_well_known_types \
                "$PROTO_FILE"
         fi
         
         find "$RUST_TMP_DIR" -name "*.rs" -exec mv {} "$SCRIPT_DIR/" \;
         
         rm -rf "$RUST_TMP_DIR"
    else
        echo "  [SKIP] Rust plugins not found."
    fi
done

echo "  Generating OpenAPI JSON..."
if command -v protoc-gen-openapiv2 &> /dev/null; then
    protoc -I "$PROTO_ROOT" -I . \
        --openapiv2_out=. \
        --openapiv2_opt=allow_merge=true,merge_file_name=identity \
        identity.proto

    if [ -f "identity.swagger.json" ]; then
        mv identity.swagger.json identity.openapi.json
    fi

    if [ -f "identity.openapi.json" ]; then
        mkdir -p "$PROTO_ROOT/gateway/src/swagger/specs"
        cp identity.openapi.json "$PROTO_ROOT/gateway/src/swagger/specs/identity.json"
        echo "  Synced gateway/src/swagger/specs/identity.json"
    fi
else
    echo "  [SKIP] protoc-gen-openapiv2 not found."
fi

echo "Done."
