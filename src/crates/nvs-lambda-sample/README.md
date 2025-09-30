# nvs-lambda-sample

Sample AWS Lambda handler for Native Vector Store that:

- Loads bundles by name (e.g., "1" or "2") from a `BUNDLES_ROOT` directory.
- Caches opened bundles across warm invocations.
- Exposes a single Hybrid search that returns full documents with a `score` field (Spring AI/LangChain style).

## Setup

1) Install cargo-lambda (local testing)

```
cargo install cargo-lambda
```

2) Place your bundles

Create a directory with two subdirectories containing `manifest.json` and other bundle files:

```
./bundles/1/manifest.json
./bundles/2/manifest.json
```

Alternatively, set `BUNDLES_ROOT` to a different path (e.g., an EFS mount in Lambda).

3) Run locally with cargo-lambda

```
export BUNDLES_ROOT=$(pwd)/bundles
export RAYON_NUM_THREADS=2

# Build and start the local Lambda runtime for this function
cargo lambda watch -p nvs-lambda-sample

# In a separate shell, invoke with a sample event (Hybrid search)
cargo lambda invoke -p nvs-lambda-sample --data-file src/crates/nvs-lambda-sample/events/hybrid_1.json
```

Response shape (JSON):

```
{
  "bundle": "1",
  "docs": [
    { "id": "doc-...", "text": "...", "metadata": { ... }, "score": 0.87 },
    { "id": "doc-...", "text": "...", "metadata": { ... }, "score": 0.76 }
  ]
}
```

## Deploying to AWS

- Zip runtime (if bundles are not included): deploy the function code, mount EFS at runtime, and set `BUNDLES_ROOT` to
  the EFS path.
- Container image: bake this binary and optionally the bundles at `/opt/bundles`, set `BUNDLES_ROOT=/opt/bundles`.
- VPC/EFS: attach VPC + SG, mount EFS to the function; use `BUNDLES_ROOT=/mnt/efs/bundles`.

Environment variables:

- `BUNDLES_ROOT` (default: `./bundles`)
- `RAYON_NUM_THREADS` (optional, to tune concurrency)

## Event schema

```
{
  "bundle": "1",
  "query": {
    "embedding": [f32, ...],
    "q": "keywords",
    "k": 10,
    "vector_weight": 0.6
  }
}
```

## Notes

- The handler lazily opens `bundle` on first use and caches it; subsequent warm invocations reuse the mmap’d data.
- For large bundles on Lambda in VPC, prefer EFS and optionally copy to `/tmp` in your own fork for the fastest page
  faults.

License: MIT
