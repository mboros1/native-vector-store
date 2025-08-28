#!/usr/bin/env python3
"""
Prepare chunk files for nvs-pack by adding required fields.
Since we don't have real embeddings yet, we'll create placeholder embeddings.
"""

import json
import os
import sys
from pathlib import Path
import random
import hashlib

def generate_placeholder_embedding(text, dimensions=384):
    """Generate a deterministic placeholder embedding based on text content."""
    # Use hash to generate deterministic but varied embeddings
    hash_obj = hashlib.md5(text.encode())
    seed = int(hash_obj.hexdigest(), 16) % (2**32)
    random.seed(seed)
    
    # Generate normalized random vector
    embedding = [random.gauss(0, 1) for _ in range(dimensions)]
    
    # Normalize to unit length
    magnitude = sum(x*x for x in embedding) ** 0.5
    if magnitude > 0:
        embedding = [x / magnitude for x in embedding]
    
    return embedding

def transform_chunks_to_documents(input_dir, output_dir, dimensions=384):
    """Transform chunk files into document format for nvs-pack."""
    
    input_path = Path(input_dir)
    output_path = Path(output_dir)
    output_path.mkdir(parents=True, exist_ok=True)
    
    chunk_files = list(input_path.glob("*_chunks.json"))
    
    if not chunk_files:
        print(f"No chunk files found in {input_dir}")
        return
    
    print(f"Processing {len(chunk_files)} chunk files...")
    
    total_documents = 0
    processed_files = 0
    
    for chunk_file in chunk_files:
        try:
            with open(chunk_file, 'r') as f:
                chunks = json.load(f)
            
            if not isinstance(chunks, list):
                print(f"Skipping {chunk_file.name}: not a list")
                continue
            
            # Create documents from chunks
            documents = []
            base_name = chunk_file.stem.replace('_chunks', '')
            
            for i, chunk in enumerate(chunks):
                if not isinstance(chunk, dict) or 'text' not in chunk:
                    continue
                
                text = chunk['text']
                
                # Create document with required fields
                doc = {
                    'id': f"{base_name}_chunk_{i}",
                    'text': text,  # Use 'text' field for compatibility
                    'metadata': {
                        'embedding': generate_placeholder_embedding(text, dimensions),
                        'source_file': base_name,
                        'chunk_index': i,
                        'original_meta': chunk.get('meta', {})
                    }
                }
                
                documents.append(doc)
            
            if documents:
                # Write to output file
                output_file = output_path / f"{base_name}.json"
                with open(output_file, 'w') as f:
                    json.dump(documents, f, separators=(',', ':'))
                
                total_documents += len(documents)
                processed_files += 1
                
                if processed_files % 50 == 0:
                    print(f"  Processed {processed_files} files, {total_documents} documents...")
        
        except Exception as e:
            print(f"Error processing {chunk_file.name}: {e}")
            continue
    
    print(f"\n✅ Conversion complete!")
    print(f"  • Processed files: {processed_files}")
    print(f"  • Total documents: {total_documents}")
    print(f"  • Output directory: {output_dir}")
    print(f"  • Embedding dimensions: {dimensions}")
    
    # Create a sample file for inspection
    if processed_files > 0:
        sample_file = next(output_path.glob("*.json"), None)
        if sample_file:
            with open(sample_file, 'r') as f:
                sample = json.load(f)
                if sample:
                    print(f"\n📝 Sample document structure:")
                    print(f"  • ID: {sample[0]['id']}")
                    print(f"  • Text length: {len(sample[0]['text'])} chars")
                    print(f"  • Embedding dims: {len(sample[0]['metadata']['embedding'])}")
                    print(f"  • Text preview: {sample[0]['text'][:100]}...")

if __name__ == "__main__":
    input_dir = sys.argv[1] if len(sys.argv) > 1 else "samples/json"
    output_dir = sys.argv[2] if len(sys.argv) > 2 else "samples/json_with_embeddings"
    dimensions = int(sys.argv[3]) if len(sys.argv) > 3 else 384
    
    transform_chunks_to_documents(input_dir, output_dir, dimensions)