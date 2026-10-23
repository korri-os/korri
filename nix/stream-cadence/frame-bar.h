/* Harvested ID contract: 1010 sync, 24-bit ID, CRC-8/ATM, inverse row.
 * Geometry scales with the output. Python bar.py owns the search, not a crop.
 */
#ifndef FRAME_BAR_H
#define FRAME_BAR_H
#include <stdint.h>
#define BAR_BITS 36
static uint8_t frame_crc(uint32_t id) {
 uint8_t crc=0;
 for(int byte=2;byte>=0;byte--) {
  crc^=(uint8_t)(id>>(byte*8));
  for(int bit=0;bit<8;bit++) crc=(uint8_t)((crc<<1)^((crc&128)?7:0));
 }
 return crc;
}
static int paint_frame_bar(uint32_t *pixels,int width,int height,uint32_t id) {
 int cell=width/(BAR_BITS+4);
 int start_x=(width-BAR_BITS*cell)/2;
 int start_y=height/4;
 if(!pixels||cell<2||height<cell*6||!id||id>0xffffff)return -1;
 uint64_t code=(UINT64_C(0xa)<<32)|((uint64_t)id<<8)|frame_crc(id);
 for(int row=0;row<2;row++)for(int bit=0;bit<BAR_BITS;bit++) {
  int white=((code>>(BAR_BITS-1-bit))&1)^row;
  uint32_t value=white?UINT32_C(0xffffffff):UINT32_C(0xff000000);
  for(int y=0;y<2*cell;y++)for(int x=0;x<cell;x++)
   pixels[(start_y+row*3*cell+y)*width+start_x+bit*cell+x]=value;
 }
 return 0;
}
#endif
