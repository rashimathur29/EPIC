#Built-in imports
import logging
from dotenv import load_dotenv
import os
from pathlib import Path
import sys
import winreg as reg
import portalocker
from PyQt5.QtWidgets import QApplication, QMessageBox
from PyQt5.QtCore import QTimer, Qt
import threading
import time
import schedule
import platform
import hashlib
import ctypes
import traceback
from concurrent.futures import ThreadPoolExecutor
from dateutil.parser import isoparse
from datetime import datetime
import requests
from functools import wraps

#Custom imports
import config
from database_manager import DatabaseManager
from user_activity_tracker import UserActivityTracker
import integration
from user_activity_gui import UserActivityGUI
from activity_monitor import ActivityMonitor

# Logger setup for entry and exit to and from functions
def log_function_call(func):
    """Decorator to log entry and exit of a function at DEBUG level."""
    logger = logging.getLogger(func.__module__)
    @wraps(func)
    def wrapper(*args, **kwargs):
        logger.debug("Entering %s", func.__name__)
        try:
            result = func(*args, **kwargs)
            logger.debug("Exiting %s", func.__name__)
            return result
        except Exception:
            logger.exception("Exception in function %s", func.__name__)
            raise
    return wrapper

# Load environment variables from .env file
load_dotenv(os.path.join(os.path.dirname(__file__), ".env"))

# Set environment variables
os.environ["_MEIPASS2"] = os.getenv("_MEIPASS2", os.path.join(os.environ["TEMP"], "productivity_temp"))
os.environ["QT_OPENGL"] = os.getenv("QT_OPENGL", "software")
os.environ["QTWEBENGINE_CHROMIUM_FLAGS"] = os.getenv("QTWEBENGINE_CHROMIUM_FLAGS", "--use-angle=swiftshader")
os.environ["QT_QUICK_BACKEND"] = os.getenv("QT_QUICK_BACKEND", "software")

# Constants
'''BASE_PATH = Path.home() / f"AppData/Local/Epic_data"
BASE_DIR = os.path.dirname(sys.executable)
VERSION_FILE_PATH = os.path.join(BASE_DIR, "version_file.txt")'''
BASE_PATH=Path("C:\AIPRUS\EPIC-kafka\AppData")
VERSION_FILE_PATH = "version.txt"

# Helper functions
def get_paths():
    """Get file paths for application data."""
    return {
        "db": BASE_PATH / "user_activity.db",
        "log": BASE_PATH / "activity.log",
        "mail": BASE_PATH / "email_detail.txt",
        "unique": BASE_PATH / "unique.txt",
        "lock": BASE_PATH / "app.lock",
        "last_cleanup": BASE_PATH / "last_cleanup_timestamp.txt",
        "user_id": BASE_PATH / "id.txt",
    }

@log_function_call
def enable_autostart(APP_NAME):
    """Enable application autostart on system boot."""
    app_path = sys.executable if getattr(sys, 'frozen', False) else os.path.abspath(__file__)
    key = reg.HKEY_CURRENT_USER
    key_path = r"Software\Microsoft\Windows\CurrentVersion\Run"
    try:
        with reg.OpenKey(key, key_path, 0, reg.KEY_WRITE) as registry_key:
            reg.SetValueEx(registry_key, APP_NAME, 0, reg.REG_SZ, f'"{app_path}"')
        logging.info(f"Autostart enabled via registry for {APP_NAME}.")
        return True
    except WindowsError as e:
        logging.error(f"Failed to enable autostart: {e}")
        return False

@log_function_call
def run_schedule(db_manager, checkout_event, gui):
    """Run scheduled tasks for data sync updates."""
    try:
        def run_threaded(job_func, *args):
            try:
                with ThreadPoolExecutor(max_workers=1) as task_executor:
                    future = task_executor.submit(job_func, *args)
                    future.result(timeout=30)  # Timeout to prevent hanging
            except Exception as e:
                logging.error(f"Error running job {job_func.__name__}: {e}", exc_info=True)
                QTimer.singleShot(0, lambda: gui.show_notification("Task Error", f"Scheduled task failed: {e}"))
        schedule.every(5).minutes.do(run_threaded, send_data_periodically, gui, db_manager, checkout_event)
        while not checkout_event.is_set():
            schedule.run_pending()
            time.sleep(1)
    except Exception as e:
        logging.error(f"Error in schedule loop: {e}", exc_info=True)
        QTimer.singleShot(0, lambda: gui.show_notification("Schedule Error", f"Schedule loop failed: {e}"))


@log_function_call
def send_data_periodically(gui, db_manager, checkout_event):
    """Send window data to the server periodically"""
    if checkout_event.is_set():
        return
    try:
        #print(db_manager.get_all_data_periodic())
        result, code, _=db_manager.get_all_data_periodic()
        if result:
            db_manager.update_last_datetime()
        else:
            if code == 403:
                logging.error("Received 403 Forbidden from server, stopping further attempts.")
                access_msg(gui)
        logging.info("Periodic data sent successfully")
    except Exception as e:
        logging.error(f"Error sending periodic data: {e}", exc_info=True)

@log_function_call
def access_msg(gui):
    """Check if the current device is the primary device."""
    try: 
        QTimer.singleShot(0, lambda: QMessageBox.warning(None, "Access Denied", "This device is not set as your primary device. You will be signed out.", QMessageBox.Ok))
        gui.auto_signout()
    except Exception as e:
        logging.error(f"Error checking device: {e}", exc_info=True)

@log_function_call
def check_for_updates(gui):
    """Check for application updates and notify GUI."""
    os_name = platform.system().lower()
    version_url = os.getenv("VERSION_URL")
    product_name = os.getenv("PRODUCT_NAME")
    if not version_url or not product_name:
        logging.error("VERSION_URL or PRODUCT_NAME environment variables not set")
        return
    try:
        response = requests.get(url=version_url, params={"productName": product_name, "platform": os_name}, timeout=5)
        response.raise_for_status()
        data = response.json()
        latest_version = data.get("latestVersion")
        deployment_datetime = data.get("deploymentDate")
    except Exception as e:
        logging.error(f"Failed to fetch version info: {e}", exc_info=True)
        return
    if not latest_version:
        logging.warning("Latest version not found in response")
        return
    try:
        deployment_date = isoparse(deployment_datetime).date() if deployment_datetime else datetime.now().date()
    except Exception as e:
        logging.error(f"Failed to parse deployment date: {e}")
        deployment_date = datetime.now().date()
    try:
        with open(VERSION_FILE_PATH, "r") as f:
            current_version = f.read().strip()
        logging.info(f"Current version: {current_version}, Latest version: {latest_version}")
    except Exception as e:
        logging.error(f"Failed to read current version file: {e}")
        return
    if compare_versions(current_version, latest_version) != 0:
        days_since_deployment = (datetime.now().date() - deployment_date).days
        logging.info(f"Days since deployment: {days_since_deployment}")
        # Enable update button if method exists
        if hasattr(gui.backend, "enableUpdateButton"):
            gui.backend.enableUpdateButton()
        if days_since_deployment > 3:
            logging.info("Showing standard update dialog")
            gui.show_update_dialog()
    else:
        logging.info("No update needed: versions match")

@log_function_call
def compare_versions(version1, version2):
    """Compare two version strings."""
    try:
        v1_parts = list(map(int, version1.split(".")))
        v2_parts = list(map(int, version2.split(".")))
        for v1, v2 in zip(v1_parts, v2_parts):
            if v1 < v2:
                return -1
            if v1 > v2:
                return 1
        return -1 if len(v1_parts) < len(v2_parts) else 1 if len(v1_parts) > len(v2_parts) else 0
    except Exception as e:
        logging.error(f"Error comparing versions: {e}", exc_info=True)
        return 0

# Entry point
def main(app, paths):
    try:
        logging.info("Starting application...")
        enable_autostart(os.getenv("APP_NAME"))
        stop_event = threading.Event()
        screenshots_dir = BASE_PATH / "screenshots"
        video_dir = BASE_PATH / "videos"
        
        db_manager = DatabaseManager(paths["db"], paths["mail"], VERSION_FILE_PATH, screenshots_dir, paths["unique"], paths["user_id"], video_dir)
        monitor = ActivityMonitor(db_manager, screenshots_dir, video_dir)
        tracker = UserActivityTracker(db_manager, monitor, VERSION_FILE_PATH)
        checkout_event = threading.Event()
        gui = UserActivityGUI(app, monitor, tracker, paths["log"], paths["mail"], VERSION_FILE_PATH, paths["unique"], db_manager,checkout_event)
        monitor.gui = gui
        with ThreadPoolExecutor(max_workers=2) as executor:
            executor.submit(run_schedule, db_manager, checkout_event, gui)
            logging.info("Starting main GUI...")
            gui.run()
            check_for_updates(gui)
            app.exec_()
            checkout_event.set()
            stop_event.set()
        logging.info("Shutting down application...")
        monitor.stop_monitoring()
        portalocker.unlock(lock_file)
        lock_file.close()
    except Exception as e:
        QTimer.singleShot(0, lambda: QMessageBox.warning(None,"Error",f"An error occurred: {e}", QMessageBox.Ok))
        logging.error(f"Application error: {e}", exc_info=True)
    finally:
        logging.info("Application resources cleaned up")

# Global exception handler
def global_exception_handler(exctype, value, tb):
    """Handle uncaught exceptions and send logs to server."""
    error_message = "".join(traceback.format_exception(exctype, value, tb))
    logging.error(f"Uncaught exception:\n{error_message}")
    try:
        paths = get_paths()
        db_manager = DatabaseManager(paths["db"], paths["mail"], VERSION_FILE_PATH, BASE_PATH / "screenshots", paths["unique"], paths["user_id"], BASE_PATH / "videos")
        with open(paths["mail"], "r") as f:
            email = f.read().strip()
        base_url = db_manager.get_single_config("base_url")
        org_id = db_manager.get_org_id()
        user_id = db_manager.get_userid()
        integration.send_logs_to_server(user_id, str(paths["log"]), email, org_id, base_url)
    except Exception as e:
        logging.error(f"Failed to send crash analytics: {e}", exc_info=True)
    for handler in logging.getLogger().handlers:
        handler.flush()
    sys.__excepthook__(exctype, value, tb)

# Entry point check
if __name__ == "__main__":
    paths = get_paths()
    config.init(paths["log"])
    try:
        logging.info("Initializing QApplication...")
        QApplication.setAttribute(Qt.AA_UseSoftwareOpenGL)
        app = QApplication(sys.argv)
        main(app, paths)
    except Exception as e:
        logging.error(f"Unexpected error during initialization: {e}", exc_info=True)
        global_exception_handler(*sys.exc_info())