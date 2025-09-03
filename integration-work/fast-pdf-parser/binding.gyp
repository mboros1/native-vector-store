{
  "targets": [
    {
      "target_name": "fast_pdf_parser",
      "sources": [
        "src/binding.cc",
        "src/fast_pdf_parser.cpp",
        "src/text_extractor.cpp",
        "src/thread_pool.cpp",
        "src/hierarchical_chunker.cpp",
        "src/cl100k_base_data.cpp"
      ],
      "include_dirs": [
        "<!@(node -p \"require('node-addon-api').include\")",
        "include"
      ],
      "dependencies": [
        "<!(node -p \"require('node-addon-api').gyp\")"
      ],
      "cflags_cc": [
        "-std=c++17",
        "-O3",
        "-Wall",
        "-Wextra"
      ],
      "libraries": [],
      "conditions": [
        ["OS=='mac'", {
          "include_dirs": [
            "/opt/homebrew/include",
            "/opt/homebrew/Cellar/mupdf-tools/1.26.3/include",
            "/usr/local/include"
          ],
          "libraries": [
            "-L/opt/homebrew/lib",
            "-L/opt/homebrew/Cellar/mupdf-tools/1.26.3/lib",
            "-L/usr/local/lib",
            "-lmupdf",
            "-lmupdf-third",
            "-lz"
          ],
          "xcode_settings": {
            "GCC_ENABLE_CPP_EXCEPTIONS": "YES",
            "CLANG_CXX_LANGUAGE_STANDARD": "c++17",
            "CLANG_CXX_LIBRARY": "libc++",
            "MACOSX_DEPLOYMENT_TARGET": "10.15",
            "OTHER_CFLAGS": [
              "-std=c++17",
              "-stdlib=libc++"
            ]
          }
        }],
        ["OS=='linux'", {
          "include_dirs": [
            "/usr/include",
            "/usr/local/include"
          ],
          "cflags_cc": [
            "-std=c++17",
            "-fPIC"
          ],
          "libraries": [
            "-lmupdf",
            "-lmupdf-third",
            "-lz",
            "-lpthread"
          ]
        }],
        ["OS=='win'", {
          "include_dirs": [
            "C:/vcpkg/installed/x64-windows/include"
          ],
          "libraries": [
            "C:/vcpkg/installed/x64-windows/lib/mupdf.lib",
            "C:/vcpkg/installed/x64-windows/lib/mupdf-third.lib"
          ],
          "msvs_settings": {
            "VCCLCompilerTool": {
              "ExceptionHandling": 1,
              "AdditionalOptions": [
                "/std:c++17"
              ]
            }
          }
        }]
      ]
    }
  ]
}