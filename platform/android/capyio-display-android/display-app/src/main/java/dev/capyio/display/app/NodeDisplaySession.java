package dev.capyio.display.app;

import android.content.Context;
import android.content.Intent;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Arrays;
import java.util.UUID;

/** One in-memory receiver lease, admitted by the authenticated Node Runtime. */
public final class NodeDisplaySession {
    public interface Events {
        void acknowledged(byte[] acknowledgement);
        void presented(long frames);
        void stopped(String failure);
    }
    private static NodeDisplaySession current;
    final String operation;
    final GameStreamLaunchRecord.Expected expected;
    private GameStreamLaunchRecord launch;
    private final Events events;
    private Runnable finish;
    private boolean closed;
    private boolean reported;

    private NodeDisplaySession(String operation, GameStreamLaunchRecord.Expected expected,
            GameStreamLaunchRecord launch, Events events) {
        this.operation = operation; this.expected = expected; this.launch = launch; this.events = events;
    }

    /** Native already verifies Session/Route/epoch, finite authorization and authenticated host. */
    public static synchronized void open(Context context, String operation, byte[] record, Events events) {
        if (current != null) throw new IllegalStateException("display receiver is busy");
        GameStreamNativeClient.requireAvailable();
        if (record.length != GameStreamLaunchRecord.RECORD_BYTES) throw new IllegalArgumentException("launch length");
        ByteBuffer b = ByteBuffer.wrap(record).order(ByteOrder.BIG_ENDIAN);
        GameStreamLaunchRecord.Expected expected = new GameStreamLaunchRecord.Expected(
                uuid(b,16),uuid(b,32),uuid(b,48),uuid(b,64),uuid(b,80),b.getLong(96),
                Arrays.copyOfRange(record,104,108),Short.toUnsignedInt(b.getShort(108)),
                b.getInt(112),b.getInt(116),b.getInt(120),b.getInt(124),b.getInt(128),b.getInt(132));
        GameStreamLaunchRecord launch = GameStreamLaunchRecord.parse(record,expected);
        NodeDisplaySession next = new NodeDisplaySession(operation,expected,launch,events);
        current = next;
        try {
            events.acknowledged(launch.acknowledgement());
        } catch (RuntimeException error) {
            next.close("receiver_open_failed"); throw error;
        }
    }
    public static synchronized void begin(Context context,String operation) {
        if(current==null||!current.operation.equals(operation)||current.closed)throw new IllegalStateException("display session ended");
        context.startActivity(new Intent(context,DisplayActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK).putExtra("nodeDisplayOperation",operation));
    }
    private static UUID uuid(ByteBuffer b,int offset) { return new UUID(b.getLong(offset),b.getLong(offset+8)); }
    static synchronized NodeDisplaySession attach(String operation,Runnable finish) {
        if (current == null || !current.operation.equals(operation) || current.closed) return null;
        current.finish = finish; return current;
    }
    synchronized GameStreamLaunchRecord takeLaunch() {
        if (closed || launch == null) throw new IllegalStateException("display launch consumed");
        GameStreamLaunchRecord value=launch; launch=null; return value;
    }
    void presented(long frames) {
        synchronized(this){if(closed||reported||frames<=0)return;reported=true;}
        events.presented(frames);
    }
    public static void stop() { NodeDisplaySession value; synchronized(NodeDisplaySession.class){value=current;} if(value!=null)value.close(""); }
    void close(String failure) {
        synchronized(this) {
            if(closed)return;
            closed=true;
            if(launch!=null){launch.close();launch=null;}
        }
        synchronized(NodeDisplaySession.class){if(current==this)current=null;}
        events.stopped(failure);
        if(finish!=null)finish.run();
    }
}
