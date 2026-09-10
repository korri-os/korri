/* Exercise the installed program's actual config_file_new/config_get_string
 * path. NUL-delimited values preserve whitespace and backslashes on stdout. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <file/config_file.h>

int main(int argc, char **argv)
{
   config_file_t *config;
   int i;
   if (argc < 3 || !(config = config_file_new(argv[1])))
      return 1;
   for (i = 2; i < argc; i++)
   {
      char *value = NULL;
      if (!config_get_string(config, argv[i], &value))
      {
         config_file_free(config);
         return 2;
      }
      fwrite(value, 1, strlen(value) + 1, stdout);
      free(value);
   }
   config_file_free(config);
   return 0;
}
