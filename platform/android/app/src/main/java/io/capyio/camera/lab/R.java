package io.capyio.camera.lab;

/** Resource compatibility shim when the proven camera sources are packaged by the unified app. */
final class R {
    static final class drawable {
        static final int ic_camera = dev.capyio.android.R.drawable.ic_capyio;
        private drawable() {}
    }

    static final class string {
        static final int app_name = dev.capyio.android.R.string.app_name;
        static final int permission_denied = dev.capyio.android.R.string.permission_denied;
        static final int camera_stopped = dev.capyio.android.R.string.camera_stopped;
        static final int opening_camera = dev.capyio.android.R.string.opening_camera;
        static final int start_camera = dev.capyio.android.R.string.start_camera;
        static final int stop_camera = dev.capyio.android.R.string.stop_camera;
        static final int camera_transport_host_hint = dev.capyio.android.R.string.camera_transport_host_hint;
        static final int camera_transport_host_rejected = dev.capyio.android.R.string.camera_transport_host_rejected;
        static final int use_front_camera = dev.capyio.android.R.string.use_front_camera;
        static final int use_back_camera = dev.capyio.android.R.string.use_back_camera;
        static final int cycle_camera_quality = dev.capyio.android.R.string.cycle_camera_quality;
        static final int inspect_camera_capabilities = dev.capyio.android.R.string.inspect_camera_capabilities;
        static final int camera_inventory_permission_required = dev.capyio.android.R.string.camera_inventory_permission_required;
        static final int camera_inventory_failed = dev.capyio.android.R.string.camera_inventory_failed;
        static final int camera_source_automatic = dev.capyio.android.R.string.camera_source_automatic;
        static final int camera_source_cycle = dev.capyio.android.R.string.camera_source_cycle;
        static final int camera_source_description = dev.capyio.android.R.string.camera_source_description;
        static final int camera_source_zoom_automatic = dev.capyio.android.R.string.camera_source_zoom_automatic;
        static final int camera_source_zoom_target = dev.capyio.android.R.string.camera_source_zoom_target;
        static final int streaming_status = dev.capyio.android.R.string.streaming_status;
        static final int camera_service_ownership_note = dev.capyio.android.R.string.camera_service_ownership_note;
        static final int camera_notification_channel = dev.capyio.android.R.string.camera_notification_channel;
        static final int camera_notification_channel_description = dev.capyio.android.R.string.camera_notification_channel_description;
        static final int camera_notification_starting = dev.capyio.android.R.string.camera_notification_starting;
        static final int camera_notification_streaming = dev.capyio.android.R.string.camera_notification_streaming;
        static final int camera_restarting = dev.capyio.android.R.string.camera_restarting;
        static final int camera_settings_selected = dev.capyio.android.R.string.camera_settings_selected;
        private string() {}
    }

    private R() {}
}
