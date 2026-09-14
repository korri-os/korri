#!/usr/bin/env nix-shell
#!nix-shell -i python3 -p python3 gcc patch
"""Execute the patched Linux CIS parser against short and valid real buffers."""
import argparse
from pathlib import Path
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('linux_source', type=Path)
    args = parser.parse_args()
    source = (args.linux_source / 'drivers/mmc/core/sdio_cis.c').read_text()
    start = source.index('static int cistpl_funce_func(')
    end = source.index('\n}\n', start) + 3
    function = source[start:end]
    # These declarations supply the kernel function's real field/API contract,
    # not a replacement parser. Every exercised branch is extracted above.
    harness = '''#include <assert.h>
#include <errno.h>
#include <stdlib.h>
#define SDIO_SDIO_REV_1_00 0
#define SDIO_SDIO_REV_1_10 1
#define MMC_CAP2_WIFI_RK912 (1U << 29)
#define HZ 1000
#define jiffies_to_msecs(x) (x)
#define mmc_hostname(x) "test"
#define pr_warn(...) ((void)0)
struct mmc_host { unsigned caps2; };
struct mmc_card { struct mmc_host *host; struct { unsigned sdio_vsn; } cccr; };
struct sdio_func { struct mmc_card *card; unsigned max_blksize, enable_timeout; };
'''
    harness += function
    harness += '''
int main(void) {
  struct mmc_host host = { .caps2 = MMC_CAP2_WIFI_RK912 };
  struct mmc_card card = { .host = &host, .cccr.sdio_vsn = 2 };
  struct sdio_func func = { .card = &card };
  for (unsigned size = 1; size < 14; size++) {
    unsigned char *buf = calloc(size, 1);
    assert(buf);
    assert(cistpl_funce_func(&card, &func, buf, size) == -EINVAL);
    free(buf);
  }
  unsigned char buf[42] = { [12] = 0x00, [13] = 0x02, [28] = 3 };
  assert(cistpl_funce_func(&card, &func, buf, 14) == 0);
  assert(func.max_blksize == 512 && func.enable_timeout == 1000);
  host.caps2 = 0;
  assert(cistpl_funce_func(&card, &func, buf, 14) == -EINVAL);
  assert(cistpl_funce_func(&card, &func, buf, 42) == 0);
  assert(func.enable_timeout == 30);
  assert(cistpl_funce_func(&card, NULL, buf, 42) == -EINVAL);
}
'''
    with tempfile.TemporaryDirectory() as temporary:
        work = Path(temporary)
        (work / 'parser.c').write_text(harness)
        subprocess.run(['cc', '-std=c11', '-Wall', '-Wextra', '-fsanitize=address,undefined', str(work / 'parser.c'), '-o', str(work / 'parser')], check=True)
        subprocess.run([str(work / 'parser')], check=True)
    print('CIS parser passed short-buffer, RK915, normal SDIO, and missing-function cases with ASan/UBSan.')


if __name__ == '__main__':
    main()
