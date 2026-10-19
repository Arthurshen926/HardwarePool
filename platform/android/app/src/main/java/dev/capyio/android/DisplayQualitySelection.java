package dev.capyio.android;

import android.content.Context;
import android.hardware.display.DisplayManager;
import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import android.view.Display;
import org.json.JSONException;
import org.json.JSONObject;

/** Startup selection only. Running sessions keep their exact negotiated format. */
final class DisplayQualitySelection {
    private DisplayQualitySelection() {}

    static JSONObject choose(Context context, JSONObject requested) throws JSONException {
        if (!requested.optBoolean("automatic")) return supported(requested) ? requested : null;
        DisplayManager manager=(DisplayManager)context.getSystemService(Context.DISPLAY_SERVICE);
        Display display=manager==null?null:manager.getDisplay(Display.DEFAULT_DISPLAY);
        if(display==null)return null;
        Display.Mode mode=display.getMode();
        int screenLong=Math.max(mode.getPhysicalWidth(),mode.getPhysicalHeight());
        int screenShort=Math.min(mode.getPhysicalWidth(),mode.getPhysicalHeight());
        int maximumFps=display.getRefreshRate()>=60.0f?60:30;
        if(display.getRefreshRate()<30.0f)return null;
        for(int[] limit:new int[][]{{2560,1600},{1920,1200},{1280,800}}) {
            double scale=Math.min(1.0,Math.min(limit[0]/(double)screenLong,limit[1]/(double)screenShort));
            int width=Math.max(2,((int)(screenLong*scale))&~1);
            int height=Math.max(2,((int)(screenShort*scale))&~1);
            for(int fps:new int[]{maximumFps,30}) {
                int bitrate=Math.max(5000,Math.min(40000,(int)Math.round(width*(double)height*fps*0.12/1000.0)));
                JSONObject selection=new JSONObject().put("width",width).put("height",height)
                        .put("refreshMillihertz",fps*1000).put("bitrateKbps",bitrate);
                if(supported(selection))return selection;
            }
        }
        return null;
    }

    private static boolean supported(JSONObject selection) throws JSONException {
        int width=selection.getInt("width"),height=selection.getInt("height");
        double rate=selection.getInt("refreshMillihertz")/1000.0;
        int bitrate=selection.getInt("bitrateKbps")*1000;
        for(MediaCodecInfo info:new MediaCodecList(MediaCodecList.REGULAR_CODECS).getCodecInfos()) {
            if(info.isEncoder())continue;
            for(String type:info.getSupportedTypes()) {
                if(!"video/avc".equalsIgnoreCase(type))continue;
                MediaCodecInfo.VideoCapabilities video=info.getCapabilitiesForType(type).getVideoCapabilities();
                if(video!=null&&video.areSizeAndRateSupported(width,height,rate)&&video.getBitrateRange().contains(bitrate))return true;
            }
        }
        return false;
    }
}
