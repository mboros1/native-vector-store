#include "fast_pdf_parser/text_extractor.h"
#include <mupdf/fitz.h>
#include <mupdf/pdf.h>
#include <stdexcept>
#include <vector>
#include <memory>
#include <iostream>

namespace fast_pdf_parser {

class TextExtractor::Impl {
public:
    Impl() {
        ctx = fz_new_context(NULL, NULL, FZ_STORE_UNLIMITED);
        if (!ctx) {
            throw std::runtime_error("Failed to create MuPDF context");
        }
        fz_register_document_handlers(ctx);
    }
    
    ~Impl() {
        if (ctx) {
            fz_drop_context(ctx);
        }
    }

    nlohmann::json extract_page(const std::string& pdf_path, int page_number, 
                               const ExtractOptions& options) {
        if (page_number % 50 == 0) {
            std::cout << "[TextExtractor::extract_page] Extracting page " << page_number << std::endl;
        }
        
        fz_document *doc = nullptr;
        fz_page *page = nullptr;
        fz_stext_page *stext = nullptr;
        
        fz_var(doc);
        fz_var(page);
        fz_var(stext);
        
        nlohmann::json result;
        
        fz_try(ctx) {
            doc = fz_open_document(ctx, pdf_path.c_str());
            if (!doc) {
                throw std::runtime_error("Failed to open PDF document");
            }
            
            int page_count = fz_count_pages(ctx, doc);
            if (page_number < 0 || page_number >= page_count) {
                throw std::out_of_range("Page number out of range");
            }
            
            page = fz_load_page(ctx, doc, page_number);
            
            // Extract structured text
            fz_stext_options opts = { 0 };
            opts.flags = FZ_STEXT_PRESERVE_LIGATURES | FZ_STEXT_PRESERVE_WHITESPACE;
            // Don't inhibit spaces - we need them for proper text extraction
            
            stext = fz_new_stext_page_from_page(ctx, page, &opts);
            
            // Convert to JSON
            result = stext_to_json(stext, options);
            result["page_number"] = page_number;
        }
        fz_always(ctx) {
            if (stext) fz_drop_stext_page(ctx, stext);
            if (page) fz_drop_page(ctx, page);
            if (doc) fz_drop_document(ctx, doc);
        }
        fz_catch(ctx) {
            throw std::runtime_error("MuPDF error during text extraction");
        }
        
        return result;
    }
    
    nlohmann::json extract_all_pages(const std::string& pdf_path,
                                    const ExtractOptions& options) {
        std::cout << "[TextExtractor::extract_all_pages] Starting extraction for all pages from " << pdf_path << std::endl;
        
        fz_document *doc = nullptr;
        fz_var(doc);
        
        nlohmann::json result;
        result["pages"] = nlohmann::json::array();
        
        std::cout << "[TextExtractor::extract_all_pages] About to open document" << std::endl;
        fz_try(ctx) {
            doc = fz_open_document(ctx, pdf_path.c_str());
            std::cout << "[TextExtractor::extract_all_pages] Document opened" << std::endl;
            if (!doc) {
                throw std::runtime_error("Failed to open PDF document");
            }
            
            int page_count = fz_count_pages(ctx, doc);
            std::cout << "[TextExtractor::extract_all_pages] Document has " << page_count << " pages" << std::endl;
            result["page_count"] = page_count;
            
            for (int i = 0; i < page_count; ++i) {
                if (i % 50 == 0) {
                    std::cout << "[TextExtractor::extract_all_pages] Processing page " << i << "/" << page_count << std::endl;
                }
                try {
                    auto page_data = extract_page(pdf_path, i, options);
                    result["pages"].push_back(page_data);
                } catch (const std::exception& e) {
                    // Log error but continue processing
                    nlohmann::json error_page;
                    error_page["page_number"] = i;
                    error_page["error"] = e.what();
                    result["pages"].push_back(error_page);
                }
            }
        }
        fz_always(ctx) {
            if (doc) fz_drop_document(ctx, doc);
        }
        fz_catch(ctx) {
            throw std::runtime_error("MuPDF error during document processing");
        }
        
        return result;
    }

private:
    nlohmann::json stext_to_json(fz_stext_page *stext, const ExtractOptions& options) {
        nlohmann::json result;
        result["blocks"] = nlohmann::json::array();
        
        for (fz_stext_block *block = stext->first_block; block; block = block->next) {
            if (block->type == FZ_STEXT_BLOCK_TEXT) {
                nlohmann::json block_json;
                block_json["type"] = "text";
                block_json["lines"] = nlohmann::json::array();
                
                if (options.extract_positions) {
                    block_json["bbox"] = bbox_to_json(block->bbox);
                }
                
                for (fz_stext_line *line = block->u.t.first_line; line; line = line->next) {
                    nlohmann::json line_json;
                    line_json["chars"] = nlohmann::json::array();
                    
                    if (options.extract_positions) {
                        line_json["bbox"] = bbox_to_json(line->bbox);
                    }
                    
                    std::string line_text;
                    
                    for (fz_stext_char *ch = line->first_char; ch; ch = ch->next) {
                        nlohmann::json char_json;
                        
                        // Convert Unicode to UTF-8
                        char utf8[5] = {0};
                        int len = fz_runetochar(utf8, ch->c);
                        utf8[len] = 0;
                        
                        char_json["char"] = std::string(utf8);
                        line_text += utf8;
                        
                        if (options.extract_positions) {
                            char_json["bbox"] = quad_to_json(ch->quad);
                            char_json["origin_x"] = ch->origin.x;
                            char_json["origin_y"] = ch->origin.y;
                        }
                        
                        if (options.extract_fonts) {
                            char_json["font"] = font_to_json(ch->font);
                            char_json["size"] = ch->size;
                        }
                        
                        // Note: color extraction not supported in current MuPDF version
                        // if (options.extract_colors && ch->color != -1) {
                        //     char_json["color"] = ch->color;
                        // }
                        
                        line_json["chars"].push_back(char_json);
                    }
                    
                    line_json["text"] = line_text;
                    block_json["lines"].push_back(line_json);
                }
                
                result["blocks"].push_back(block_json);
            }
        }
        
        return result;
    }
    
    nlohmann::json bbox_to_json(const fz_rect& bbox) {
        return {
            {"x0", bbox.x0},
            {"y0", bbox.y0},
            {"x1", bbox.x1},
            {"y1", bbox.y1}
        };
    }
    
    nlohmann::json quad_to_json(const fz_quad& quad) {
        return {
            {"ul_x", quad.ul.x}, {"ul_y", quad.ul.y},
            {"ur_x", quad.ur.x}, {"ur_y", quad.ur.y},
            {"ll_x", quad.ll.x}, {"ll_y", quad.ll.y},
            {"lr_x", quad.lr.x}, {"lr_y", quad.lr.y}
        };
    }
    
    nlohmann::json font_to_json(fz_font *font) {
        if (!font) return nullptr;
        
        nlohmann::json font_json;
        const char *name = fz_font_name(ctx, font);
        font_json["name"] = name ? name : "unknown";
        font_json["is_bold"] = fz_font_is_bold(ctx, font);
        font_json["is_italic"] = fz_font_is_italic(ctx, font);
        font_json["is_monospace"] = fz_font_is_monospaced(ctx, font);
        
        return font_json;
    }
    
public:
    int get_page_count(const std::string& pdf_path) {
        std::cout << "[TextExtractor::get_page_count] Getting page count for " << pdf_path << std::endl;
        
        fz_document *doc = nullptr;
        fz_var(doc);
        
        int page_count = 0;
        
        fz_try(ctx) {
            doc = fz_open_document(ctx, pdf_path.c_str());
            if (!doc) {
                throw std::runtime_error("Failed to open PDF document");
            }
            
            page_count = fz_count_pages(ctx, doc);
            std::cout << "[TextExtractor::get_page_count] Document has " << page_count << " pages" << std::endl;
        }
        fz_always(ctx) {
            if (doc) fz_drop_document(ctx, doc);
        }
        fz_catch(ctx) {
            throw std::runtime_error("MuPDF error getting page count");
        }
        
        return page_count;
    }
    
    fz_context *ctx;
};

TextExtractor::TextExtractor() : pImpl(std::make_unique<Impl>()) {}
TextExtractor::~TextExtractor() = default;

nlohmann::json TextExtractor::extract_page(const std::string& pdf_path, int page_number, 
                                          const ExtractOptions& options) {
    return pImpl->extract_page(pdf_path, page_number, options);
}

nlohmann::json TextExtractor::extract_all_pages(const std::string& pdf_path,
                                               const ExtractOptions& options) {
    return pImpl->extract_all_pages(pdf_path, options);
}

int TextExtractor::get_page_count(const std::string& pdf_path) {
    return pImpl->get_page_count(pdf_path);
}

} // namespace fast_pdf_parser