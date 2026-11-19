// Offline validator against the user's actual standalone Island implementation.
// Build: c++ -std=c++17 -I /path/to/island scripts/check-island-parser.cpp -o /tmp/check-island
// Run: /tmp/check-island /tmp/generated-island.css
// Does not instantiate Island, connect to Wayland, load behavior, or send IPC.
#include "config.hpp"
#include <fstream>
#include <iterator>
#include <unistd.h>

int main(int argc, char** argv) {
    if (argc != 2 && argc != 3) return 2;
    std::ifstream file(argv[1]);
    if (!file) return 2;
    std::string css((std::istreambuf_iterator<char>(file)), {});
    Settings settings{};
    std::string error;
    // The real parser warns, but returns success, for unknown properties,
    // selectors and bad colors. Turn every such diagnostic into failure.
    FILE* diagnostics = tmpfile();
    int saved_stderr = dup(STDERR_FILENO);
    if (!diagnostics || saved_stderr < 0) return 2;
    fflush(stderr);
    if (dup2(fileno(diagnostics), STDERR_FILENO) < 0) return 2;
    bool loaded = load_settings("", argv[1], settings, error);
    fflush(stderr);
    long diagnostic_bytes = ftell(diagnostics);
    if (dup2(saved_stderr, STDERR_FILENO) < 0) return 2;
    close(saved_stderr);
    fclose(diagnostics);
    if (!loaded || diagnostic_bytes != 0) return 1;
    const Style& style = settings.style;
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
    if (count < 11) return 1;
    if (argc == 3) {
        if (strcmp(argv[2], "--dump")) return 2;
        auto color = [](const char* name, const RGBA& c) {
            printf("%s %.9f %.9f %.9f %.9f\n", name, c.r, c.g, c.b, c.a);
        };
        color("background", style.bg); color("border", style.border);
        color("time", style.time.color); color("date", style.date.color);
        color("text", style.text.color); color("title", style.title.color);
        color("body", style.body.color); color("label", style.label.color);
        color("value", style.value.color); color("flash", style.flash.color);
        color("dnd", style.dnd.color); color("track", style.track);
        color("fill", style.fill); color("tile", style.tile); color("tile-on", style.tile_on);
        printf("sizes %.9f %.9f %.9f %.9f %.9f %.9f %.9f %.9f %.9f\n",
          style.border_w, style.time.size, style.date.size, style.text.size, style.title.size,
          style.body.size, style.label.size, style.value.size, style.flash.size);
    }
    return 0;
}
