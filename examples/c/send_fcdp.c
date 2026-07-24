/* Send a valid FCDP v0.1 test access unit using POSIX sockets. */
#include <arpa/inet.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>

static uint16_t crc16(const uint8_t *bytes, size_t length) {
  uint16_t value = 0xffff;
  for (size_t i = 0; i < length; ++i) { value ^= (uint16_t)bytes[i] << 8; for (int bit = 0; bit < 8; ++bit) value = value & 0x8000 ? (uint16_t)((value << 1) ^ 0x1021) : (uint16_t)(value << 1); }
  return value;
}

int main(int argc, char **argv) {
  if (argc != 4) { fprintf(stderr, "usage: %s <ipv4-address> <port> <text>\n", argv[0]); return 2; }
  size_t length = strlen(argv[3]); if (length > 1163) return 2;
  uint8_t packet[1200] = { 'F', 'C', 1, 3, 0, 0 };
  packet[13] = 1; /* session_id = 1 in network byte order */
  uint16_t two = htons(1), deadline = htons(1000), payload = htons((uint16_t)length); uint32_t sequence = htonl(1);
  memcpy(packet + 14, &two, 2); memcpy(packet + 16, &two, 2); memcpy(packet + 18, &sequence, 4); memcpy(packet + 22, &sequence, 4); memcpy(packet + 28, &two, 2); memcpy(packet + 31, &deadline, 2); memcpy(packet + 33, &payload, 2);
  uint16_t check = htons(crc16(packet, 35)); memcpy(packet + 35, &check, 2); memcpy(packet + 37, argv[3], length);
  int fd = socket(AF_INET, SOCK_DGRAM, 0); struct sockaddr_in address = { .sin_family = AF_INET, .sin_port = htons((uint16_t)atoi(argv[2])) };
  if (inet_pton(AF_INET, argv[1], &address.sin_addr) != 1 || sendto(fd, packet, 37 + length, 0, (struct sockaddr *)&address, sizeof(address)) < 0) return 1;
  return 0;
}
