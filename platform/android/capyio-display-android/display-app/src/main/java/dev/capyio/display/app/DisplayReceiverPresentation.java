package dev.capyio.display.app;

import java.util.Locale;

/** Labels distinguish negotiated settings from measured presentation counters. */
public final class DisplayReceiverPresentation {
    private DisplayReceiverPresentation() {}
    public static String failure(String code) {
        if(code==null||code.isEmpty())return "";
        if(code.startsWith("receiver_ended_"))return "屏幕视频连接已中断；请检查两端网络后重新开始（"+code+"）。";
        if(code.startsWith("receiver_failed_")||"receiver_open_failed".equals(code))return "画面接收器启动或解码失败；请尝试较低画质后重新开始（"+code+"）。";
        return "屏幕共享已停止："+code;
    }
    static String metrics(int width,int height,int refresh,int bitrate,long frames,long deltaFrames,long elapsed) {
        String measured=elapsed>0&&deltaFrames>=0?String.format(Locale.ROOT," · 实测呈现 %.1f FPS",deltaFrames*1000.0/elapsed):"";
        return width+" × "+height+String.format(Locale.ROOT," · 协商 %.1f FPS / %.1f Mbps",refresh/1000.0,bitrate/1000.0)
                +measured+" · 已呈现 "+frames+" 帧";
    }
}
