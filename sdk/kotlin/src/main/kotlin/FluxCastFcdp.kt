package fluxcast

import java.nio.ByteBuffer
import java.nio.ByteOrder

data class FcdpHeader(val packetType: Int=3,val flags:Int=0,val sessionId:Long=1,val streamId:Int=1,val epoch:Int=0,val sequence:Long=1,val frameId:Long=1,val fragmentIndex:Int=0,val fragmentCount:Int=1,val priority:Int=0,val deadlineMs:Int=1000)
object FluxCastFcdp {
 const val HEADER_LEN=37; const val MAX_DATAGRAM=1200
 private fun crc16(data:ByteArray):Int { var v=0xffff; for(x in data){v=v xor ((x.toInt()and 255)shl 8); repeat(8){v=if(v and 0x8000!=0)((v shl 1)xor 0x1021)and 0xffff else (v shl 1)and 0xffff}};return v }
 fun encode(h:FcdpHeader,payload:ByteArray):ByteArray { require(payload.size+HEADER_LEN<=MAX_DATAGRAM&&h.priority in 0..3&&h.fragmentCount>0&&h.fragmentIndex in 0 until h.fragmentCount); val p=ByteBuffer.allocate(HEADER_LEN+payload.size).order(ByteOrder.BIG_ENDIAN);p.put(byteArrayOf(70,67,1,h.packetType.toByte(),h.flags.toByte(),0));p.putLong(h.sessionId);p.putShort(h.streamId.toShort());p.putShort(h.epoch.toShort());p.putInt(h.sequence.toInt());p.putInt(h.frameId.toInt());p.putShort(h.fragmentIndex.toShort());p.putShort(h.fragmentCount.toShort());p.put(h.priority.toByte());p.putShort(h.deadlineMs.toShort());p.putShort(payload.size.toShort());val raw=p.array().copyOfRange(0,35);p.putShort(crc16(raw).toShort());p.put(payload);return p.array() }
 fun decode(packet:ByteArray):Pair<FcdpHeader,ByteArray> { require(packet.size>=HEADER_LEN&&packet[0].toInt()==70&&packet[1].toInt()==67&&packet[2].toInt()==1);require(crc16(packet.copyOfRange(0,35))==ByteBuffer.wrap(packet,35,2).order(ByteOrder.BIG_ENDIAN).short.toInt()and 0xffff);val b=ByteBuffer.wrap(packet).order(ByteOrder.BIG_ENDIAN);b.position(3);val t=b.get().toInt()and 255;val f=b.get().toInt()and 255;b.get();val h=FcdpHeader(t,f,b.long,b.short.toInt()and 65535,b.short.toInt()and 65535,b.int.toLong()and 0xffffffffL,b.int.toLong()and 0xffffffffL,b.short.toInt()and 65535,b.short.toInt()and 65535,b.get().toInt()and 255,b.short.toInt()and 65535);val n=b.short.toInt()and 65535;require(n==packet.size-HEADER_LEN&&h.priority<=3&&h.fragmentCount>0&&h.fragmentIndex<h.fragmentCount);return h to packet.copyOfRange(HEADER_LEN,packet.size) }
}
