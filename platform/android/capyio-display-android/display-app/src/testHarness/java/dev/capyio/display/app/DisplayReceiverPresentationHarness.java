package dev.capyio.display.app;

public final class DisplayReceiverPresentationHarness {
    public static void main(String[] args) {
        String first=DisplayReceiverPresentation.metrics(1920,1080,60000,15000,1,1,0);
        if(!first.contains("协商 60.0 FPS / 15.0 Mbps")||first.contains("实测"))throw new AssertionError(first);
        String next=DisplayReceiverPresentation.metrics(1920,1080,60000,15000,26,25,500);
        if(!next.contains("实测呈现 50.0 FPS"))throw new AssertionError(next);
        if(!DisplayReceiverPresentation.failure("receiver_ended_-1").contains("网络"))throw new AssertionError();
        if(!DisplayReceiverPresentation.failure("").isEmpty())throw new AssertionError();
        System.out.println("Display receiver presentation: 4 assertions passed");
    }
}
