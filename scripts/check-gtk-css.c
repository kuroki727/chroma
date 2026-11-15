// Headless GTK3 stylesheet validation; does not initialize a display or Waybar.
// Build with the flags printed by: pkg-config --cflags --libs gtk+-3.0
#include <gtk/gtk.h>
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    GtkCssProvider *provider = gtk_css_provider_new();
    GError *error = NULL;
    gboolean ok = gtk_css_provider_load_from_path(provider, argv[1], &error);
    if (error) { g_printerr("%s\n", error->message); g_error_free(error); }
    g_object_unref(provider);
    return ok ? 0 : 1;
}
