// Regression for d9ed107c6: FIONREAD has _IOC_NONE but still takes a pointer.
#include <sys/ioctl.h>
#include <unistd.h>

int main(void) {
  int descriptors[2], pending = -1;
  char bytes[4];
  if (pipe(descriptors) || write(descriptors[1], "test", 4) != 4) return 1;
  if (ioctl(descriptors[0], FIONREAD, &pending) || pending != 4) return 2;
  if (read(descriptors[0], bytes, sizeof(bytes)) != 4) return 3;
  return 0;
}
