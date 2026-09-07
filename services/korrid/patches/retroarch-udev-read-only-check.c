/* Exercise extracted upstream C with deterministic open/ioctl outcomes.
 * Successful opens use real descriptors: access modes and cleanup are not
 * simulated. No controller, root access, ACL changes or event injection needed.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <linux/input.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <unistd.h>
#include <libretro.h>

#define MAX_USERS 1
#define NAME_MAX_LENGTH 256
#define INLINE inline
#define DEFAULT_RUMBLE_GAIN 75

static int open_errors[2];
static unsigned opens, ioctls, writes, errors, ff_queries, ff_uploads, connects;
static int last_fd;
static bool key_support, query_failure;
static const char *joystick_path = "/dev/input/event9";

static int recording_open(const char *path, int flags)
{
   assert(strcmp(path, joystick_path) == 0);
   assert(opens < 2);
   assert(flags == ((opens == 0 ? O_RDWR : O_RDONLY) | O_NONBLOCK));
   int error = open_errors[opens++];
   if (error)
   {
      errno = error;
      return -1;
   }
   last_fd = open("/dev/null", flags);
   assert(last_fd >= 0);
   return last_fd;
}

static void set_bit(unsigned long *bits, unsigned bit)
{
   bits[bit / (sizeof(long) * CHAR_BIT)] |=
      1UL << (bit % (sizeof(long) * CHAR_BIT));
}

static int recording_ioctl(int fd, unsigned long request, void *data)
{
   assert(fd == last_fd);
   assert(fcntl(fd, F_GETFL) >= 0);
   ioctls++;
   if (_IOC_NR(request) == _IOC_NR(EVIOCGBIT(0, 0)))
   {
      if (query_failure)
      {
         errno = EIO;
         return -1;
      }
      if (key_support)
         set_bit(data, EV_KEY);
   }
   else if (_IOC_NR(request) == _IOC_NR(EVIOCGBIT(EV_KEY, 0)) ||
            _IOC_NR(request) == _IOC_NR(EVIOCGBIT(EV_ABS, 0)))
      memset(data, 0, _IOC_SIZE(request));
   else if (_IOC_NR(request) == _IOC_NR(EVIOCGNAME(0)))
      snprintf(data, _IOC_SIZE(request), "Xbox gameplay controller");
   else if (request == EVIOCGID ||
            _IOC_NR(request) == _IOC_NR(EVIOCGPHYS(0)) ||
            _IOC_NR(request) == _IOC_NR(EVIOCGUNIQ(0)))
      memset(data, 0, _IOC_SIZE(request));
   else if (_IOC_NR(request) == _IOC_NR(EVIOCGBIT(EV_FF, 0)))
   {
      ff_queries++;
      set_bit(data, FF_RUMBLE);
   }
   else if (request == EVIOCGEFFECTS)
   {
      ff_queries++;
      *(int *)data = 2;
   }
   else if (request == EVIOCSFF)
   {
      assert((fcntl(fd, F_GETFL) & O_ACCMODE) == O_RDWR);
      ff_uploads++;
      ((struct ff_effect *)data)->id = 0;
   }
   else
      assert(!"unexpected ioctl in extracted upstream function");
   return 0;
}

static ssize_t recording_write(int fd, const void *data, size_t size)
{
   const struct input_event *event = data;
   assert(fd == last_fd);
   assert((fcntl(fd, F_GETFL) & O_ACCMODE) == O_RDWR);
   assert(size == sizeof(*event));
   assert(event->type == EV_FF);
   writes++;
   return (ssize_t)size;
}

/* Only the unrelated frontend/configuration calls are fixture implementations. */
struct udev_device;
typedef struct { struct { unsigned input_rumble_gain; } uints; } settings_t;
static settings_t *config_get_ptr(void) { return NULL; }
static const char *input_config_get_device_name(unsigned pad)
{
   assert(pad == 0);
   return NULL;
}
static const struct { const char *ident; } udev_joypad = { "udev" };
static void input_autoconfigure_connect(const char *name, const char *display,
      const char *phys, const char *driver, unsigned pad, int32_t vid, int32_t pid)
{
   assert(strcmp(name, "Xbox gameplay controller") == 0);
   assert(display == NULL && phys[0] == '\0');
   assert(strcmp(driver, "udev") == 0 && pad == 0 && vid == 0 && pid == 0);
   connects++;
}
#define string_is_empty(s) (!(s) || !(s)[0])
#define strlcpy(dst, src, size) snprintf(dst, size, "%s", src)
#define RARCH_LOG(...) ((void)0)
#define RARCH_ERR(...) ((void)errors++)
#define open recording_open
#define ioctl recording_ioctl
#define write recording_write
#include "udev-joypad-extracted.h"
#undef open
#undef ioctl
#undef write

static void reset(int first_error, int second_error)
{
   memset(udev_pads, 0, sizeof(udev_pads));
   udev_pads[0].fd = -1;
   open_errors[0] = first_error;
   open_errors[1] = second_error;
   opens = ioctls = writes = errors = ff_queries = ff_uploads = connects = 0;
   last_fd = -1;
   key_support = true;
   query_failure = false;
}

static void connected_controller(int first_error)
{
   int fd;
   unsigned before;
   reset(first_error, 0);
   /* A stale permission errno must not make a successful RW open retry. */
   errno = EACCES;
   fd = udev_open_joystick(joystick_path);
   assert(fd >= 0 && opens == (first_error ? 2U : 1U));
   assert(ioctls == 3);
   assert(udev_add_pad(NULL, 0, fd, joystick_path) == 1);
   assert(connects == 1 && udev_pads[0].fd == fd);
   assert(udev_pads[0].writable == !first_error);
   if (first_error)
   {
      assert((fcntl(fd, F_GETFL) & O_ACCMODE) == O_RDONLY);
      assert(ff_queries == 0 && writes == 0);
      assert(udev_pads[0].num_effects == 0);
      /* Even stale effect state or equal gain must not claim rumble support. */
      udev_pads[0].num_effects = 2;
      before = ioctls;
      for (unsigned i = 0; i < 100; i++)
      {
         assert(!udev_set_rumble_gain(0, 0));
         assert(!udev_set_rumble_gain(0, 75));
         assert(!udev_set_rumble(0, RETRO_RUMBLE_STRONG, 1234));
         assert(!udev_set_rumble(0, RETRO_RUMBLE_WEAK, 1234));
         assert(!udev_set_rumble(0, RETRO_RUMBLE_STRONG, 0));
      }
      assert(ioctls == before && writes == 0 && ff_uploads == 0 && errors == 0);
   }
   else
   {
      assert(ff_queries == 2 && writes == 1);
      assert(udev_set_rumble_gain(0, 50));
      assert(udev_set_rumble(0, RETRO_RUMBLE_STRONG, 1234));
      assert(udev_set_rumble(0, RETRO_RUMBLE_STRONG, 0));
      assert(udev_set_rumble(0, RETRO_RUMBLE_WEAK, 1234));
      assert(ff_uploads == 2 && writes == 5 && errors == 0);
   }
   udev_free_pad(0);
   assert(fcntl(fd, F_GETFL) == -1 && errno == EBADF);
   assert(!udev_pads[0].writable && udev_pads[0].fd == -1);
   assert(!udev_set_rumble_gain(0, 50));
   assert(!udev_set_rumble(0, RETRO_RUMBLE_STRONG, 1234));
}

int main(void)
{
   const int permissions[] = { EACCES, EPERM };
   const int unrelated[] = { ENOENT, ENODEV, EIO, EMFILE, EINTR };
   connected_controller(0);
   for (unsigned i = 0; i < sizeof(permissions) / sizeof(*permissions); i++)
   {
      connected_controller(permissions[i]);
      for (unsigned j = 0; j < sizeof(permissions) / sizeof(*permissions); j++)
      {
         reset(permissions[i], permissions[j]);
         assert(udev_open_joystick(joystick_path) == -1);
         assert(errno == permissions[j] && opens == 2 && ioctls == 0);
      }
   }
   for (unsigned i = 0; i < sizeof(unrelated) / sizeof(*unrelated); i++)
   {
      reset(unrelated[i], 0);
      assert(udev_open_joystick(joystick_path) == -1);
      assert(errno == unrelated[i] && opens == 1 && ioctls == 0);
   }
   /* Neither a failed capability query nor missing EV_KEY becomes a pad.
    * Both RW and RO failures close the descriptor without another open. */
   for (unsigned ro = 0; ro < 2; ro++)
      for (unsigned failed_query = 0; failed_query < 2; failed_query++)
      {
         reset(ro ? EACCES : 0, 0);
         key_support = false;
         query_failure = failed_query;
         assert(udev_open_joystick(joystick_path) == -1);
         assert(opens == (ro ? 2U : 1U));
         assert(fcntl(last_fd, F_GETFL) == -1 && errno == EBADF);
      }
   puts("RetroArch udev read-only open, validation, rumble and cleanup checks passed");
   return 0;
}
