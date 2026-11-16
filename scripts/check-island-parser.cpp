// Offline validator against the user's actual standalone Island implementation.
// Build: c++ -std=c++17 -I /path/to/island scripts/check-island-parser.cpp -o /tmp/check-island
// Run: /tmp/check-island /tmp/generated-island.css
// Does not instantiate Island, connect to Wayland, load behavior, or send IPC.
#include "config.hpp"
#include <fstream>
#include <iterator>

int main(int argc, char** argv) {
    if (argc != 2) return 2;
    std::ifstream file(argv[1]);
    if (!file) return 2;
    std::string css((std::istreambuf_iterator<char>(file)), {});
    Style style{};
    std::string error;
    if (!apply_style(css, style, error)) return 1;
    for (size_t start; (start = css.find("/*")) != std::string::npos;) {
        auto end = css.find("*/", start + 2);
        if (end == std::string::npos) return 1;
        css.erase(start, end + 2 - start);
    }
    // Island logs invalid colors without returning false. Independently pass
    // every color declaration through the same parse_color implementation.
    size_t pos = 0;
    unsigned count = 0;
    while ((pos = css.find(':', pos)) != std::string::npos) {
        auto end = css.find(';', pos);
        if (end == std::string::npos) return 1;
        auto value = trim(css.substr(pos + 1, end - pos - 1));
        if (!value.empty() && (value[0] == '#' || value.compare(0, 3, "rgb") == 0)) {
            RGBA color;
            if (!parse_color(value, color)) return 1;
            ++count;
        }
        pos = end + 1;
    }
    return count >= 11 ? 0 : 1;
}
