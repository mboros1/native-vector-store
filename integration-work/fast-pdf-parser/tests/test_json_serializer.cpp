#include <gtest/gtest.h>
#include <fast_pdf_parser/json_serializer.h>

class JsonSerializerTest : public ::testing::Test {
protected:
    nlohmann::json CreateMockDocument() {
        nlohmann::json doc;
        doc["content"]["doc_items"] = nlohmann::json::array();
        
        // Add some mock text blocks
        for (int i = 0; i < 3; ++i) {
            nlohmann::json item;
            item["type"] = "text_block";
            item["page_number"] = i;
            item["text"] = "This is a sample text block " + std::to_string(i) + 
                          ". It contains some content for testing purposes.";
            item["bbox"] = {{"x0", 100}, {"y0", 100}, {"x1", 500}, {"y1", 200}};
            doc["content"]["doc_items"].push_back(item);
        }
        
        doc["meta"] = {
            {"schema_name", "docling_core.transforms.chunker.DocMeta"},
            {"version", "1.0.0"},
            {"origin", {
                {"mimetype", "application/pdf"},
                {"binary_hash", 12345},
                {"filename", "test.pdf"},
                {"uri", nullptr}
            }},
            {"doc_items", doc["content"]["doc_items"]},
            {"headings", nlohmann::json::array()},
            {"captions", nullptr}
        };
        
        return doc;
    }
};

TEST_F(JsonSerializerTest, ToDoclingFormat) {
    nlohmann::json raw_output;
    raw_output["pages"] = nlohmann::json::array();
    
    // Create a simple page with blocks
    nlohmann::json page;
    page["page_number"] = 0;
    page["blocks"] = nlohmann::json::array();
    
    nlohmann::json block;
    block["type"] = "text";
    block["lines"] = nlohmann::json::array();
    
    nlohmann::json line;
    line["text"] = "Hello, World!";
    line["chars"] = nlohmann::json::array();
    
    block["lines"].push_back(line);
    page["blocks"].push_back(block);
    raw_output["pages"].push_back(page);
    
    auto result = fast_pdf_parser::JsonSerializer::to_docling_format(
        raw_output, "test.pdf", 12345);
    
    EXPECT_TRUE(result.contains("content"));
    EXPECT_TRUE(result.contains("meta"));
    EXPECT_EQ(result["meta"]["origin"]["filename"], "test.pdf");
    EXPECT_EQ(result["meta"]["origin"]["binary_hash"], 12345);
    EXPECT_EQ(result["meta"]["schema_name"], "docling_core.transforms.chunker.DocMeta");
}

TEST_F(JsonSerializerTest, ChunkDocument) {
    auto doc = CreateMockDocument();
    
    auto chunks = fast_pdf_parser::JsonSerializer::chunk_document(doc, 100);
    
    EXPECT_FALSE(chunks.empty());
    
    for (const auto& chunk : chunks) {
        EXPECT_TRUE(chunk.contains("text"));
        EXPECT_TRUE(chunk.contains("meta"));
        EXPECT_FALSE(chunk["text"].get<std::string>().empty());
        
        // Verify metadata structure
        EXPECT_EQ(chunk["meta"]["schema_name"], "docling_core.transforms.chunker.DocMeta");
        EXPECT_TRUE(chunk["meta"].contains("doc_items"));
    }
}

TEST_F(JsonSerializerTest, ChunkDocumentRespectMaxTokens) {
    auto doc = CreateMockDocument();
    
    // Create a document with larger text
    doc["content"]["doc_items"].clear();
    for (int i = 0; i < 10; ++i) {
        nlohmann::json item;
        item["type"] = "text_block";
        item["text"] = std::string(200, 'A'); // 200 characters ~ 50 tokens
        doc["content"]["doc_items"].push_back(item);
    }
    
    auto chunks = fast_pdf_parser::JsonSerializer::chunk_document(doc, 100);
    
    // Should have multiple chunks due to token limit
    EXPECT_GT(chunks.size(), 1);
    
    // Each chunk should respect the token limit
    for (const auto& chunk : chunks) {
        std::string text = chunk["text"];
        // Rough estimate: 4 chars per token
        EXPECT_LE(text.length() / 4, 120); // Allow some overflow
    }
}

TEST_F(JsonSerializerTest, SerializeChunks) {
    auto doc = CreateMockDocument();
    auto chunks = fast_pdf_parser::JsonSerializer::chunk_document(doc, 512, true);
    
    std::string serialized = fast_pdf_parser::JsonSerializer::serialize_chunks(chunks);
    
    // Parse back to verify it's valid JSON
    nlohmann::json parsed;
    ASSERT_NO_THROW(parsed = nlohmann::json::parse(serialized));
    
    EXPECT_TRUE(parsed.is_array());
    EXPECT_EQ(parsed.size(), chunks.size());
}

TEST_F(JsonSerializerTest, ExtractHeadings) {
    nlohmann::json raw_output;
    raw_output["pages"] = nlohmann::json::array();
    
    nlohmann::json page;
    page["page_number"] = 0;
    page["blocks"] = nlohmann::json::array();
    
    // Add a heading-like block
    nlohmann::json heading_block;
    heading_block["type"] = "text";
    heading_block["lines"] = {{{"text", "Introduction"}}};
    page["blocks"].push_back(heading_block);
    
    // Add a regular text block
    nlohmann::json text_block;
    text_block["type"] = "text";
    text_block["lines"] = {{{"text", "This is a regular paragraph with punctuation."}}};
    page["blocks"].push_back(text_block);
    
    raw_output["pages"].push_back(page);
    
    auto result = fast_pdf_parser::JsonSerializer::to_docling_format(
        raw_output, "test.pdf", 12345);
    
    EXPECT_TRUE(result["meta"].contains("headings"));
    auto headings = result["meta"]["headings"];
    
    // Should detect "Introduction" as a heading
    bool found_heading = false;
    for (const auto& heading : headings) {
        if (heading == "Introduction") {
            found_heading = true;
            break;
        }
    }
    EXPECT_TRUE(found_heading);
}

TEST_F(JsonSerializerTest, EmptyDocument) {
    nlohmann::json empty_doc;
    empty_doc["content"]["doc_items"] = nlohmann::json::array();
    empty_doc["meta"] = {
        {"schema_name", "docling_core.transforms.chunker.DocMeta"},
        {"version", "1.0.0"}
    };
    
    auto chunks = fast_pdf_parser::JsonSerializer::chunk_document(empty_doc, 512, true);
    EXPECT_TRUE(chunks.empty());
}