{
  "targets": [
    {
      "target_name": "vector_store",
      "sources": ["src/binding.cc", "src/vector_store_loader.cpp", "src/vector_store_loader_mmap.cpp", "src/vector_store_loader_adaptive.cpp"],
      "include_dirs": [
        "<!@(node -p \"require('node-addon-api').include\")",
        "src"
      ],
      "dependencies": ["<!(node -p \"require('node-addon-api').gyp\")"],
      "cflags_cc": [
        "-std=c++17",
        "-O3",
        "-march=native",
        "-fno-exceptions"
      ],
      "defines": ["NAPI_DISABLE_CPP_EXCEPTIONS"],
      "conditions": [
        ["OS=='mac'", {
          "include_dirs": ["/opt/homebrew/opt/libomp/include", "/opt/homebrew/include"],
          "xcode_settings": {
            "GCC_ENABLE_CPP_EXCEPTIONS": "NO",
            "OTHER_CFLAGS": ["-Xpreprocessor", "-fopenmp"],
            "OTHER_CPLUSPLUSFLAGS": ["-Xpreprocessor", "-fopenmp"],
            "OTHER_LDFLAGS": ["-L/opt/homebrew/opt/libomp/lib", "-lomp"]
          },
          "libraries": ["-L/opt/homebrew/opt/libomp/lib", "-lomp", "-L/opt/homebrew/lib", "-lsimdjson"]
        }],
        ["OS=='linux'", {
          "cflags_cc": ["-fopenmp"],
          "libraries": ["-lgomp", "-lsimdjson"]
        }],
        ["OS=='win'", {
          "msvs_settings": {
            "VCCLCompilerTool": {
              "ExceptionHandling": 0,
              "OpenMP": "true"
            }
          },
          "libraries": ["simdjson.lib"]
        }]
      ]
    }
  ]
}