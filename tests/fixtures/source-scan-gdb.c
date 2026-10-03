/* Test-only GDB stand-in: exercises scan timeout, cancellation and diagnostics. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>
int main(void) {
    FILE *pid = fopen("scan.pid", "w");
    if (!pid) return 1;
    fprintf(pid, "%lu", (unsigned long)GetCurrentProcessId());
    fclose(pid);
    const char *mode = getenv("DEBUGTUI_SCAN_TEST_MODE");
    if (mode && !strcmp(mode, "empty")) {
        puts("1^done\n2^done,files=[]\n3^exit");
    } else if (mode && !strcmp(mode, "error")) {
        puts("1^error,msg=\"fixture invalid ELF\"\n3^exit");
    } else {
        Sleep(60000);
    }
    return 0;
}
